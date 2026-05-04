use std::net::SocketAddr;

use anyhow::{Context, Result};
use bytes::{Buf, BytesMut};
use log::{debug, error, info, warn};
use socket2::{Domain, Protocol, Socket, TcpKeepalive, Type};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::{TcpListener, TcpStream},
};

use crate::config::client::Client;
use crate::database::core::DB;
use crate::command::{
    dispatcher::dispatch,
    command::extract_command,
};
use crate::resp::{
    parser::{Parser, ParseError},
    types::RespType,
};

// Tuning knobs

// Write-side kernel socket buffer (bytes). Raising this reduces send syscalls
// for large responses but consumes kernel memory per connection.
const WRITE_BUF_BYTES: usize = 8 * 1024; // 8 KiB

// Initial capacity of the per-connection read buffer (bytes). The buffer grows
// automatically if a single command is larger than this.
const READ_BUF_BYTES: usize = 512; // 512 bytes

// Keep-alive idle time before the first probe (seconds). Catches dead peers
// that vanish without sending a FIN/RST.
const TCP_KEEPALIVE_IDLE_SECS: u64 = 60;

// `Server` is a thin wrapper that holds everything needed to start listening.
// Call [`Server::run`] to block and serve connections forever.
//
// ## Architecture
// Instead of a single accept loop (which is a hard bottleneck above ~100k RPS)
// we create one 'TcpListener' per logical CPU using 'SO_REUSEPORT'.
// The Linux kernel distributes incoming SYNs across the listeners using a flow-based hash,
// so accept contention is eliminated entirely without any userspace coordination.
// Each acceptor task runs in Tokio's multi-thread scheduler and spawns an independent task per
// connection, identical to the old design, but now with N parallel accept paths.
pub struct Server {
    addr: SocketAddr,
    db: DB,
}

impl Server {
    // Bind to 'port' on localhost and load (or create) the on-disk snapshot.
    // Returns an error if the address cannot be parsed or the port is already in use.
    pub async fn new(port: &str) -> Result<Self> {
        let addr: SocketAddr = format!("127.0.0.1:{}", port)
            .parse()
            .with_context(|| format!("invalid port: {}", port))?;

        let db = DB::new();

        Ok(Self { addr, db })
    }

    // Spawn one acceptor task per logical CPU core, then wait for all of them
    // (they run forever unless a fatal error occurs).
    // This function blocks the caller until the server shuts down.
    pub async fn run(self) -> Result<()> {
        let num_workers = num_cpus::get();
        info!(
            "Starting {} acceptor task(s) on {}",
            num_workers, self.addr
        );

        let mut handles = Vec::with_capacity(num_workers);

        for worker_id in 0..num_workers {
            // Each worker builds its own TcpListener from a socket2::Socket
            // so we can set SO_REUSEPORT before bind().
            let listener = build_listener(self.addr)
                .with_context(|| format!("worker {} failed to bind", worker_id))?;

            let db = self.db.clone();

            let handle = tokio::spawn(async move {
                accept_loop(worker_id, listener, db).await;
            });

            handles.push(handle);
        }

        info!("Ember is listening on {}", self.addr);

        // Wait for all acceptors.
        for h in handles {
            if let Err(e) = h.await {
                error!("Acceptor task panicked: {:?}", e);
            }
        }

        Ok(())
    }
}

// Internal helpers

// Build a non-blocking 'TcpListener' with 'SO_REUSEPORT' enabled.
// 'SO_REUSEPORT' lets multiple sockets bind to the same (address, port) pair.
// so each acceptor task gets its own dedicated socket, no userspace mutex.
fn build_listener(addr: SocketAddr) -> Result<TcpListener> {
    let domain = if addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };

    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
        .context("socket() failed")?;

    // Allow multiple sockets to bind to the same port.
    socket
        .set_reuse_port(true)
        .context("SO_REUSEPORT not supported on this platform")?;

    // Allow rapid server restarts without waiting for TIME_WAIT to drain.
    socket
        .set_reuse_address(true)
        .context("SO_REUSEADDR failed")?;

    // Put the socket in non-blocking mode before handing it to Tokio's reactor.
    socket
        .set_nonblocking(true)
        .context("O_NONBLOCK failed")?;

    socket
        .bind(&addr.into())
        .with_context(|| format!("bind() to {} failed", addr))?;

    // Backlog of 1024 is higher than the default 128. Under a connection
    // burst this gives the kernel more room to queue SYNs before rejecting
    // them with RST.
    socket.listen(1024).context("listen() failed")?;

    let std_listener: std::net::TcpListener = socket.into();
    TcpListener::from_std(std_listener).context("Tokio TcpListener conversion failed")
}

// Accepts connections in a loop and spawns a task for each one.
// A transient `accept()` error (e.g. `EMFILE`, `ENFILE`) is logged and retried.
// A persistent error on the listener itself (very rare) is logged and causes the worker to exit.
async fn accept_loop(worker_id: usize, listener: TcpListener, db: DB) {
    loop {
        match listener.accept().await {
            Ok((socket, peer_addr)) => {
                let db = db.clone();
                tokio::spawn(async move {
                    debug!("[w{}] connection from {}", worker_id, peer_addr);
                    handle_client(socket, &db).await;
                });
            }
            Err(e) => {
                // EMFILE / ENFILE / ECONNABORTED are transient; keep going.
                // If the listener fd itself is broken we will loop very fast —
                // a short sleep prevents a busy-loop that would pin the CPU.
                error!("[w{}] accept error: {}", worker_id, e);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    }
}

// Serve all commands arriving on 'socket' until the peer closes the connection
// or a fatal I/O error occurs.
//
// Socket options applied here:
// - TCP_NODELAY: disables Nagle's algorithm so that small RESP responses
//   (e.g. `+OK\r\n`) are sent immediately rather than being held by the kernel
//   for up to 200 ms waiting for more data to coalesce.
// - TCP keep-alive: detects dead peers (e.g. clients that vanish without
//   sending FIN) so that their task slots are reclaimed promptly.
//
// Pipelining: A single `read_buf()` may deliver multiple back-to-back RESP commands.
// The inner loop drains all complete commands before yielding back to the outer
// read, so pipeline throughput is never gated on an extra read syscall.
async fn handle_client(socket: TcpStream, db: &DB) {
    if let Err(e) = socket.set_nodelay(true) {
        warn!("TCP_NODELAY failed: {}", e);
    }

    let ka = TcpKeepalive::new()
        .with_time(std::time::Duration::from_secs(TCP_KEEPALIVE_IDLE_SECS));
    let sock_ref = socket2::SockRef::from(&socket);
    if let Err(e) = sock_ref.set_tcp_keepalive(&ka) {
        warn!("TCP_KEEPALIVE failed: {}", e);
    }

    // Split into independent read and write halves so BufWriter can own the
    // write side while we simultaneously read into `buf`.
    let (mut reader, writer) = socket.into_split();

    // BufWriter coalesces multiple small `write_all()` calls into a single
    // `send()` syscall. For a pipeline of 10 commands this means one syscall
    // instead of ten. This is a major win on high-throughput workloads.
    let mut writer = BufWriter::with_capacity(WRITE_BUF_BYTES, writer);
    let mut buf = BytesMut::with_capacity(READ_BUF_BYTES);
    let mut client = Client::new();

    loop {
        // Append new bytes to whatever is left over from the previous read.
        let bytes_read = match reader.read_buf(&mut buf).await {
            Ok(n) => n,
            Err(e) => {
                error!("read error: {}", e);
                break;
            }
        };

        // n == 0 means the peer sent EOF (graceful close).
        if bytes_read == 0 {
            break;
        }

        // inner pipeline drain loop
        // We process every complete command already in `buf` before issuing
        // another read(). This is the key to pipeline throughput: a client
        // that sends 50 commands in one TCP segment gets all 50 replies in
        // one BufWriter flush, not 50 round-trips.
        loop {
            if buf.is_empty() {
                break;
            }

            let (resp, consumed) = match Parser::parse(&buf) {
                Ok((data, consumed)) => (data, consumed),

                Err(ParseError::Incomplete) => {
                    // Not enough bytes yet; go back to the outer read loop.
                    break;
                }

                Err(ParseError::Invalid(msg)) => {
                    error!("invalid RESP: {}", msg);
                    let _ = RespType::SimpleError(msg).write_to(&mut writer).await;
                    // Discard the buffer; we cannot know where the next
                    // valid command starts in a corrupted stream.
                    buf.clear();
                    break;
                }
            };

            // Consume exactly the bytes that formed this command.
            buf.advance(consumed);

            let cmd = match extract_command(&resp) {
                Ok(cmd) => cmd,
                Err(e) => {
                    error!("extract_command: {}", e);
                    let _ = RespType::SimpleError(e.to_string()).write_to(&mut writer).await;
                    continue;
                }
            };

            let res = match dispatch(&mut client, db, cmd) {
                Ok(res) => res,
                Err(e) => {
                    let _ = RespType::SimpleError(e.to_string()).write_to(&mut writer).await;
                    continue;
                }
            };

            if let Err(e) = res.write_to(&mut writer).await {
                error!("write error: {}", e);
                // The connection is broken; stop processing for this client.
                return;
            }
        }

        // Flush once per read batch. BufWriter may have accumulated several
        // responses; flushing here sends them all in a single syscall.
        if let Err(e) = writer.flush().await {
            error!("flush error: {}", e);
            return;
        }
    }
}

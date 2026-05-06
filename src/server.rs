use std::net::SocketAddr;

use anyhow::{Context, Result};
use bytes::{Buf, BytesMut};
use log::{debug, error, info, warn};
use socket2::{Domain, Protocol, Socket, TcpKeepalive, Type};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::{TcpListener, TcpStream},
};

use crate::command::{command::extract_command, dispatcher::dispatch};
use crate::config::client::Client;
use crate::database::core::DB;
use crate::resp::{
    parser::{ParseError, Parser},
    types::RespType,
};

// Tuning knobs

// Write-side kernel socket buffer (bytes). 64 KiB fits a full pipeline batch of
// ~3,000 small GET replies without a mid-batch flush, cutting flush syscalls by ~8x
// compared to the old 8 KiB default.
const WRITE_BUF_BYTES: usize = 64 * 1024; // 64 KiB

// Initial capacity of the per-connection read buffer (bytes). 16 KiB avoids the
// repeated grow-and-reallocate churn that 4 KiB causes when clients send moderately
// large values or pipelined batches that exceed the initial allocation.
const READ_BUF_BYTES: usize = 16 * 1024; // 16 KiB

// Hint to the kernel for the per-socket send buffer. Linux doubles the value
// internally (the extra half is for bookkeeping), so 256 KiB yields ~512 KiB
// effective. This lets a single flush() push far more data before back-pressure
// forces the task to yield and be rescheduled.
const SO_SNDBUF_BYTES: usize = 256 * 1024; // 256 KiB hint -> ~512 KiB effective

// Keep-alive idle time before the first probe (seconds). Catches dead peers
// that vanish without sending a FIN/RST.
const TCP_KEEPALIVE_IDLE_SECS: u64 = 60;

// Maximum number of commands processed in one pipeline drain pass before the
// inner loop flushes and yields. Without this cap a single client that sends
// 500k commands in one TCP segment monopolises the Tokio worker thread
// indefinitely — it never hits an .await so the scheduler never preempts it.
// Flushing mid-batch also delivers partial results to the client sooner.
const MAX_PIPELINE_DEPTH: usize = 1_000;

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
    pub async fn new(mut port: u16) -> Result<Self> {
        let db = DB::new();

        loop {
            let addr: SocketAddr = format!("127.0.0.1:{}", port)
                .parse()
                .with_context(|| format!("invalid port: {}", port))?;

            // Try to bind a listener to check if the port is available.
            // Since we use SO_REUSEPORT in build_listener, this will fail
            // if another application (without SO_REUSEPORT) is using the port.
            match build_listener(addr) {
                Ok(_) => {
                    return Ok(Self { addr, db });
                }
                Err(e) => {
                    // Check if the error is Address In Use
                    let is_in_use = e.chain().any(|cause| {
                        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                            io_err.kind() == std::io::ErrorKind::AddrInUse
                        } else {
                            false
                        }
                    });

                    if is_in_use {
                        warn!("Port {} is already in use, trying {}", port, port + 1);
                        port += 1;
                    } else {
                        return Err(e).context(format!("failed to bind to port {}", port));
                    }
                }
            }
        }
    }

    // Intended for use in tests only. Do not call in production code.
    #[allow(dead_code)]
    pub fn addr(&self) -> SocketAddr {
        self.addr
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
// - SO_SNDBUF: raises the kernel send buffer so that large pipeline flushes
//   do not block waiting for the socket to drain.
//
// Pipelining: A single `read_buf()` may deliver multiple back-to-back RESP commands.
// The inner loop drains all complete commands before yielding back to the outer
// read, so pipeline throughput is never gated on an extra read syscall.
// A depth cap (MAX_PIPELINE_DEPTH) ensures a single client cannot monopolise
// a worker thread by sending an unbounded burst in one TCP segment.
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
    if let Err(e) = sock_ref.set_send_buffer_size(SO_SNDBUF_BYTES) {
        warn!("SO_SNDBUF failed: {}", e);
    }
    drop(sock_ref);

    let (mut reader, writer) = socket.into_split();
    let mut writer = BufWriter::with_capacity(WRITE_BUF_BYTES, writer);
    let mut buf = BytesMut::with_capacity(READ_BUF_BYTES);
    let mut client = Client::new();

    'outer: loop {
        if buf.is_empty() {
            // Buffer is empty: we must await new data.
            match reader.read_buf(&mut buf).await {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => {
                    error!("read error: {}", e);
                    break;
                }
            }
        } else {
            match reader.try_read_buf(&mut buf) {
                Ok(0) => break, // EOF
                Ok(_) => {}     // got more bytes, fall through to parse
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => { error!("read error: {}", e); break; }
            }
        }

        if buf.capacity().saturating_sub(buf.len()) < READ_BUF_BYTES / 2 {
            buf.reserve(READ_BUF_BYTES);
        }

        let mut pipeline_count = 0usize;
        let mut made_progress = false;

        loop {
            if buf.is_empty() {
                break;
            }

            let (resp, consumed) = match Parser::parse(&buf) {
                Ok((data, consumed)) => (data, consumed),
                Err(ParseError::Incomplete) => break,
                Err(ParseError::Invalid(msg)) => {
                    error!("invalid RESP: {}", msg);
                    let _ = RespType::SimpleError(msg).write_to(&mut writer).await;
                    let _ = writer.flush().await;
                    buf.clear();
                    if buf.capacity() < READ_BUF_BYTES {
                        buf.reserve(READ_BUF_BYTES - buf.capacity());
                    }
                    break;
                }
            };

            buf.advance(consumed);
            pipeline_count += 1;
            made_progress = true;

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
                return;
            }

            if pipeline_count >= MAX_PIPELINE_DEPTH {
                if let Err(e) = writer.flush().await {
                    error!("flush error (mid-pipeline): {}", e);
                    break 'outer;
                }
                pipeline_count = 0;
            }
        }

        if made_progress && !writer.buffer().is_empty() {
            if let Err(e) = writer.flush().await {
                error!("flush error: {}", e);
                return;
            }
        }
    }

    let _ = writer.flush().await;
}



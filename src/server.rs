use std::net::SocketAddr;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;

use crate::config::client::Client;
use crate::database::core::DB;

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
                    let client = Client::new();
                    client.handle(socket, &db).await;
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

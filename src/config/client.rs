use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};

use bytes::Buf;

use tokio::net::TcpStream;
use bytes::BytesMut;
use log::{error, warn};
use socket2::TcpKeepalive;

use crate::database::core::DB;
use crate::resp::{
    parser::{ParseError, Parser},
    types::RespType,
};
use crate::command::command::Command;
use crate::command::{command::extract_command, dispatcher::dispatch};

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

#[derive(Clone)]
pub struct Client {
    pub in_transaction: bool,
    pub queued_commands: Vec<Command>,
}

impl Client {
    pub fn new() -> Client {
        Client {
            in_transaction: false,
            queued_commands: Vec::new(),
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
    pub async fn handle(&self, socket: TcpStream, db: &DB) {
        if let Err(e) = socket.set_nodelay(true) {
            warn!("TCP_NODELAY failed: {}", e);
        }

        let ka =
            TcpKeepalive::new().with_time(std::time::Duration::from_secs(TCP_KEEPALIVE_IDLE_SECS));
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
                    Err(e) => {
                        error!("read error: {}", e);
                        break;
                    }
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
                        let _ = RespType::SimpleError(e.to_string())
                            .write_to(&mut writer)
                            .await;
                        continue;
                    }
                };

                let res = match dispatch(&mut client, db, cmd) {
                    Ok(res) => res,
                    Err(e) => {
                        let _ = RespType::SimpleError(e.to_string())
                            .write_to(&mut writer)
                            .await;
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
}

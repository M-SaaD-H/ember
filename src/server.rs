use anyhow::{Error, Result};
use bytes::{Buf, BytesMut};
use log::{error, info, warn};
use tokio::{
    // AsyncWriteExt trait provides asynchronous write methods like write_all
    io::{AsyncWriteExt, AsyncReadExt},
    net::{TcpListener, TcpStream},
};

use crate::config::client::Client;
use crate::command::{
    dispatcher::dispatch,
    command::extract_command, 
};
use crate::resp::{
    parser::{Parser, ParseError},
    types::RespType
};

#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
}

impl Server {
    // Creates a new server instance bound to the given port.

    // Returns an error if the TCP listener cannot be created (e.g. the port
    // is already in use or the process lacks the required privileges).
    pub async fn new(port: &str) -> Result<Self> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await.map_err(|e| {
            error!("Failed to bind to {}: {}", addr, e);
            Error::from(e)
        })?;

        info!("TCP listener started on port: {}", port);
        Ok(Self { listener })
    }

    // Runs the server in an infinite loop, continuously handling
    // incoming connections
    pub async fn run(&mut self) -> Result<()> {
        loop {
            let socket = match self.accept_connection().await {
                Ok(stream) => stream,
                Err(e) => {
                    // A transient accept failure should not take down the whole
                    // server.  Log it and keep accepting.
                    error!("Error accepting connection: {}", e);
                    continue;
                }
            };

            // Spawns a new async task to handle the connection
            // This allows the server to handle multiple connections concurrently
            tokio::spawn(async move {
                match socket.peer_addr() {
                    Ok(addr) => {
                        info!("New connection from: {}", addr);
                        Server::handle_client(socket, addr.port()).await
                    },
                    Err(e) => warn!("New connection (peer address unavailable: {})", e),
                }
            });
        }
    }
    
    async fn handle_client(mut socket: TcpStream, client_id: u16) {
        // read the TCP message and store the raw bytes in the buffer
        let mut buf = BytesMut::with_capacity(512);
        let mut client = Client::new(client_id);

        loop {
            // Read data from the socket into the buffer.
            // This appends to any leftover bytes from a previous incomplete parse.
            let bytes_read = match socket.read_buf(&mut buf).await {
                Ok(n) => n,
                Err(e) => {
                    error!("Error reading from socket: {}", e);
                    break;
                }
            };

            // Client closed the connection.
            if bytes_read == 0 {
                break;
            }

            // Inner loop: drain all complete commands from the buffer.
            // A single TCP read may contain multiple pipelined commands,
            // and we must process all of them before reading again.
            loop {
                if buf.is_empty() {
                    break;
                }

                // Try to parse a complete RESP message from the buffer
                let (resp, consumed) = match Parser::parse(&buf) {
                    Ok((data, consumed)) => (data, consumed),
                    Err(ParseError::Incomplete) => {
                        // Buffer doesn't contain a full command yet.
                        // Break to the outer loop to read more data from the socket.
                        break;
                    }
                    Err(ParseError::Invalid(msg)) => {
                        error!("Invalid RESP data: {}", msg);
                        if let Err(e) = socket.write_all(&RespType::SimpleError(msg).to_bytes()).await {
                            error!("Error writing error response to client: {}", e);
                        }
                        // Clear the buffer to avoid getting stuck on malformed data
                        buf.clear();
                        break;
                    }
                };

                // Advance past the consumed bytes so the buffer now starts
                // at the next command (if any)
                buf.advance(consumed);

                let cmd = match extract_command(&resp) {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        error!("Failed to extract command: {}", e);
                        if let Err(e) = socket.write_all(&RespType::SimpleError(e.to_string()).to_bytes()).await {
                            error!("Error writing error response to client: {}", e);
                        }
                        continue;
                    }
                };

                let res = match dispatch(&mut client, cmd) {
                    Ok(res) => res,
                    Err(e) => {
                        if let Err(e) = socket.write_all(&RespType::SimpleError(e.to_string()).to_bytes()).await {
                            error!("Error writing error response to client: {}", e);
                        }
                        continue;
                    }
                };

                if let Err(e) = socket.write_all(&res.to_bytes()).await {
                    error!("Error writing response to client: {}", e);
                    // The connection is broken; stop processing commands for this client.
                    return;
                }
            }
        }
    }

    // Accepts an incoming TCP connection and returns the corresponding stream
    async fn accept_connection(&mut self) -> Result<TcpStream> {
        // `.accept()` returns a tuple (TcpStream, SocketAddr);
        // we only need the stream
        let (stream, _) = self.listener.accept().await.map_err(Error::from)?;
        Ok(stream)
    }
}

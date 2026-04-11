use anyhow::{Error, Result};
use bytes::{Buf, BytesMut};
use log::{error, info};
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
    // Create a new server instance on the given port
    pub async fn new(port: String) -> Self {
        let addr = format!("127.0.0.1:{}", port);
        let listener = match TcpListener::bind(addr).await {
            Ok(listener) => {
                info!("TCP listener started on port: {}", port);
                listener
            },
            Err(e) => {
                error!("{}", e);
                panic!("Error initializing the server.");
            }
        };

        Self {
            listener
        }
    }

    // Runs the server in an infinite loop continiously handling
    // incoming connections
    pub async fn run(&mut self) -> Result<()> {        
        loop {
            // accpet the incoming connections
            let socket = match self.accept_connection().await {
                Ok(stream) => stream,
                Err(e) => {
                    // Log the error and panic the thread
                    // this will crash the server if there is an error
                    // connecting to the client
                    error!("{}", e);
                    panic!("Error accpeting connection.");
                }
            };

            // Spawns a new async task to handle the connection
            // This allows the server to handle multiple connections concurrently
            tokio::spawn(async move {
                info!("New connection from port: {}", socket.peer_addr().unwrap().port());
                Server::handle_client(socket).await
            });
        }
    }
    
    async fn handle_client(mut socket: TcpStream) {
        // read the TCP message and store the raw bytes in the buffer
        let mut buf = BytesMut::with_capacity(512);
        let mut client = Client::new();

        loop {
            // Read data from the socket into the buffer.
            // This appends to any leftover bytes from a previous incomplete parse.
            let bytes_read = match socket.read_buf(&mut buf).await {
                Ok(n) => n,
                Err(e) => {
                    error!("Error reading request: {}", e);
                    break;
                }
            };

            // Client closed connection
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
                            error!("Error writing to the client. E: {}", e);
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
                        error!("E: {}", e);
                        if let Err(e) = socket.write_all(&RespType::SimpleError(e.to_string()).to_bytes()).await {
                            error!("Error writing to the client. E: {}", e);
                        }
                        continue;
                    }
                };

                let res = match dispatch(&mut client, cmd) {
                    Ok(res_str) => res_str,
                    Err(e) => {
                        if let Err(e) = socket.write_all((e.to_string() + "\r\n").as_bytes()).await {
                            error!("{}", e);
                            panic!("Error writing response to the client.");
                        }
                        return;
                    }
                };

                if let Err(e) = socket.write_all(&res.to_bytes()).await {
                    error!("{}", e);
                    panic!("Error writing response to the client.");
                }
            }
        }
    }

    // Accepts the incoming the TCP connection and returns the
    // corrosponding tokio TcpStream
    async fn accept_connection(&mut self) -> Result<TcpStream> {
        // loop is used to retry connection untill it is success
        loop {
            // '.accept()' returns a tuple (TcpStream, SocketAddr)
            // but we only need the stream
            match self.listener.accept().await {
                Ok((stream, _)) => return Ok(stream),
                Err(e) => return Err(Error::from(e)),
            }
        }
    }
}

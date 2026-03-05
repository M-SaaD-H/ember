use anyhow::{Error, Result};
use bytes::BytesMut;
use log::{error, info};
use tokio::{
    // AsyncWriteExt trait provides asynchronous write methods like write_all
    io::{AsyncWriteExt, AsyncReadExt},
    net::{TcpListener, TcpStream},
};

use crate::resp::{
    parser::Parser,
    types::RespType
};

#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
}

impl Server {
    // Create a new server instance on the given port
    pub async fn new(port: u16) -> Self {
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
            let mut socket = match self.accept_connection().await {
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
                // read the TCP message and store the raw bytes in the buffer
                let mut buf = BytesMut::with_capacity(512);
                if let Err(e) = socket.read_buf(&mut buf).await {
                    panic!("Error reading request: {}", e);
                }

                // parse the RESP data from the the buffer
                let resp_data = match Parser::parse(buf) {
                    Ok((data, _)) => data,
                    Err(e) => RespType::SimpleError(format!("{}", e)),
                };

                if let Err(e) = socket.write_all(&resp_data.to_bytes()).await {
                    error!("{}", e);
                    panic!("Error writing response to the client.");
                }
            });
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

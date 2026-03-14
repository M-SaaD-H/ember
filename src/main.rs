mod server;
mod resp;
mod command;
mod config;
mod database;

use anyhow::Result;

use crate::server::Server;

// constants for the server
const DEFAULT_PORT: u16 = 6379;

fn parse_args() -> String {
    let args: Vec<String> = std::env::args().collect();
    
    let mut port = DEFAULT_PORT.to_string();

    // skipping the first arg (first arg is not of our interest)
    // the args that are passed on while starting the program
    // starts from the second position.
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                if i + 1 < args.len() {
                    port = args[i + 1].clone();
                    i += 2;
                } else {
                    i += 1; // there is no ++ operator in rust
                }
            }
            // Add more args here
            _ => i += 1
        }
    }

    port
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let port = parse_args();

    // await the 'new()' cz initializing the server takes time
    let mut server = Server::new(port).await;
    server.run().await?;
    // the server will keep running untill the program is terminated

    // technically unreached (dead end code) but required
    // to satisfy the Result return type of main()
    Ok(())
}

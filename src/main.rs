mod server;
mod resp;

use anyhow::Result;

use crate::server::Server;


#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let port = 6379;

    // await the 'new()' cz initializing the server takes time
    let mut server = Server::new(port).await;
    server.run().await?;
    // the server will keep running untill the program is terminated

    // technically unreached (dead end code) but required
    // to satisfy the Result return type of main()
    Ok(())
}

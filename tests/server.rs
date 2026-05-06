//! Integration tests for the Ember server: binding, accept loop, and networking.
//!
//! Currently focuses on the port auto-increment logic (trying port + 1 if
//! the requested port is already in use).

mod common;

use ember::server::Server;
use std::net::TcpListener;

// Binding & Port selection

#[tokio::test]
async fn test_port_auto_increment_on_conflict() {
    // Bind to a random available port to guarantee it's "in use"
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    // Now try to create a Server on that same port.
    // It should detect the conflict and pick port + 1 (or higher if port + 1 is also taken).
    let server = Server::new(port).await.expect("Server::new failed");
    
    assert!(
        server.addr().port() > port,
        "Server should have picked a higher port. Expected > {}, got {}",
        port,
        server.addr().port()
    );
    
    // Check that the original port is still held by our listener (proving the server didn't steal it)
    assert_eq!(
        listener.local_addr().unwrap().port(),
        port,
        "Original listener port should not have been affected"
    );
}

//! A loopback port for an embedded Vera node's JSON-RPC methods.
//!
//! The hosted node binds the address it is given and does not tell its host
//! which address it chose, so a test cannot hand it port 0 and learn the
//! port, nor hand it a listener it already holds. A free port is found here
//! and released before the node binds it; another test can take it in
//! between. That start fails to bind and is made again on a new port. No
//! other failure is retried.

use std::future::Future;
use std::net::SocketAddr;

/// Starts made before a lost port fails the test.
const ATTEMPTS: usize = 8;

/// What `start` builds, its embedded Vera node serving JSON-RPC on the
/// returned loopback address.
pub(crate) async fn on_free_rpc_port<T, F, Fut>(mut start: F) -> (T, SocketAddr)
where
    F: FnMut(SocketAddr) -> Fut,
    Fut: Future<Output = defra_node::Result<T>>,
{
    let mut attempt = 1;
    loop {
        let rpc = free_port();
        match start(rpc).await {
            Ok(started) => return (started, rpc),
            Err(error) if attempt < ATTEMPTS && lost_the_port(&error) => attempt += 1,
            Err(error) => panic!("node: {error:?}"),
        }
    }
}

/// A loopback port no listener held when it was asked for.
fn free_port() -> SocketAddr {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("free loopback port")
}

/// Whether the start failed because the Vera node could not bind its RPC
/// address.
fn lost_the_port(error: &defra_node::Error) -> bool {
    matches!(error, defra_node::Error::EmbeddedVera(host) if host.is_rpc_bind())
}

/// A start on a port another listener holds fails as a lost port.
#[test]
fn a_held_port_is_a_lost_port() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let refused = super::starting(
            directory.path(),
            defra_node::EmbeddedVeraActor::Secp256k1Key(vec![42; 32]),
            Vec::new(),
            Some(held.local_addr().unwrap()),
        )
        .await;
        match refused {
            Err(error) => assert!(lost_the_port(&error), "{error:?}"),
            Ok(_) => panic!("a node started on a port another listener holds"),
        }
    })
    .unwrap();
}

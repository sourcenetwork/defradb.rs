//! Binding a listener already held by another process.

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

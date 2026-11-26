//! Identity from the shared seed, records, and dial by id.

use std::time::Duration;

use super::{connect, loopback, run, seed, TestNode, IO_TIMEOUT};
use crate::pinc::{PincEndpointConfig, SERVICE_POINTER_LEN};
use crate::transport::{P2PTransport, PeerId};

#[test]
fn the_peer_id_is_the_hex_of_the_seed_public_key_and_signatures_verify() {
    run(async {
        let seed = seed();
        let expected = ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key();
        let node = TestNode::with_config(PincEndpointConfig::new(seed, loopback())).await;
        assert_eq!(node.id.as_str(), hex::encode(expected.to_bytes()));
        assert_eq!(node.transport.local_public_key_proto(), expected.to_bytes());
        let signature = node.transport.sign(b"message").unwrap();
        let signature = ed25519_dalek::Signature::from_slice(&signature).unwrap();
        expected.verify_strict(b"message", &signature).unwrap();
        node.shutdown().await;
    });
}

/// A node keeps its peer id when it moves between iroh and pinc: both
/// transports, started from one seed, report the same id.
#[cfg(feature = "iroh-transport")]
#[test]
fn the_same_seed_is_the_same_peer_id_on_iroh() {
    run(async {
        let seed = seed();
        let secret_key = crate::iroh::secret_key_from_bytes(*seed);
        let (command_tx, _events, _replicators, iroh_task) = crate::iroh::spawn_endpoint(
            crate::iroh::IrohEndpointConfig {
                secret_key: secret_key.clone(),
                node_identity: None,
                relay_mode: crate::iroh::IrohRelayModeConfig::Disabled,
                discovery: crate::iroh::IrohDiscoveryConfig::Disabled,
                bind_port: None,
                bind_addr: Some(loopback().ip()),
                max_concurrent_multipath_paths: None,
                allowlist: crate::iroh::IrohAllowlistConfig::AcceptAll,
            },
            edge::testing::edge(),
        )
        .await
        .unwrap();
        let iroh = crate::iroh::IrohTransport::new(command_tx, secret_key);
        let pinc = TestNode::with_config(PincEndpointConfig::new(seed, loopback())).await;
        assert_eq!(iroh.local_peer_id(), pinc.transport.local_peer_id());
        assert_eq!(
            iroh.local_public_key_proto(),
            pinc.transport.local_public_key_proto()
        );
        pinc.shutdown().await;
        drop(iroh_task);
    });
}

#[test]
fn a_peer_reads_the_service_record_pointer_another_published() {
    run(async {
        let a = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        let pointer = *b"defra/service-records/1:0123456789abcd";
        assert_eq!(pointer.len(), SERVICE_POINTER_LEN);
        b.transport.set_service_pointer(pointer).await.unwrap();
        connect(&a.transport, &b.transport).await;
        let seen = crate::rt::time::timeout(IO_TIMEOUT, async {
            loop {
                if let Some(seen) = a.transport.service_pointer(&b.id).await.unwrap() {
                    return seen;
                }
                crate::rt::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("b's record reaches a on the connection");
        assert_eq!(seen, pointer);
        a.shutdown().await;
        b.shutdown().await;
    });
}

#[test]
fn a_peer_seen_once_is_dialed_by_id_alone() {
    run(async {
        let a = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        connect(&a.transport, &b.transport).await;
        a.transport.disconnect(&b.id).await.unwrap();
        crate::rt::time::timeout(IO_TIMEOUT, async {
            while a.transport.connected_peers().await.unwrap().contains(&b.id) {
                crate::rt::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        a.transport.dial(&b.id, Vec::new()).await.unwrap();
        a.transport
            .poll_until_connected(&b.id, IO_TIMEOUT)
            .await
            .unwrap();
        a.shutdown().await;
        b.shutdown().await;
    });
}

#[test]
fn an_unknown_peer_cannot_be_dialed_by_id() {
    run(async {
        let a = TestNode::spawn().await;
        let stranger = PeerId::new(hex::encode([7u8; 32]));
        assert!(a.transport.dial(&stranger, Vec::new()).await.is_err());
        a.shutdown().await;
    });
}

#[test]
fn path_info_and_network_change() {
    run(async {
        let a = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        connect(&a.transport, &b.transport).await;
        let info = a.transport.peer_connection(&b.id).await.unwrap().unwrap();
        assert_eq!(info.path, "direct");
        assert_eq!(info.dialed_by_us, Some(true));
        assert!(info.remote_address.is_some());
        a.transport.network_change().await.unwrap();
        assert!(a.transport.connected_peers().await.unwrap().contains(&b.id));
        a.shutdown().await;
        b.shutdown().await;
    });
}

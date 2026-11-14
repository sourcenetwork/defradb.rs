//! Identity from the shared seed, records, and dial by id.

use std::time::Duration;

use super::{connect, loopback, run, seed, TestNode, IO_TIMEOUT};
use crate::transport::{P2PTransport, PeerId};
use crate::vinc::{VincEndpointConfig, SERVICE_POINTER_LEN};

#[test]
fn the_peer_id_is_the_hex_of_the_seed_public_key_and_signatures_verify() {
    run(async {
        let seed = seed();
        let expected = ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key();
        let node = TestNode::with_config(VincEndpointConfig::new(seed, loopback())).await;
        assert_eq!(node.id.as_str(), hex::encode(expected.to_bytes()));
        assert_eq!(node.transport.local_public_key_proto(), expected.to_bytes());
        let signature = node.transport.sign(b"message").unwrap();
        let signature = ed25519_dalek::Signature::from_slice(&signature).unwrap();
        expected.verify_strict(b"message", &signature).unwrap();
        node.shutdown().await;
    });
}

/// A node keeps its peer id when it moves between iroh and vinc.
#[cfg(feature = "iroh-transport")]
#[test]
fn the_same_seed_is_the_same_peer_id_on_iroh() {
    let seed = seed();
    let iroh_id = iroh::SecretKey::from_bytes(&seed).public().to_string();
    let vinc_id = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&seed)
            .verifying_key()
            .to_bytes(),
    );
    assert_eq!(iroh_id, vinc_id);
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

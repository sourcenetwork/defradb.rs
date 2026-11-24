//! A raw request (a KMS key request) that reaches a peer through an iroh
//! gossip relay names its publisher as `source`, not the relay (#2773).
//!
//! iroh-gossip's `delivered_from` is the last hop and its content is
//! unsigned, so `source` has to come from the payload. The hosts form a line,
//! asker - relay - responder: the ends allow only the relay in and deny each
//! other, so every copy the responder gets was relayed.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use iroh::SecretKey;

use crate::iroh::{
    spawn_endpoint, IrohAllowlistConfig, IrohDiscoveryConfig, IrohEndpointConfig,
    IrohRelayModeConfig, IrohTransport,
};
use crate::rt::time::timeout;
use crate::topics::{DefraTopic, ENCRYPTION_TOPIC};
use crate::transport::{P2PTransport, PeerId, TransportEvent};

type Events = crate::sync::DispatchReceiver<TransportEvent<iroh::endpoint::SendStream>>;

fn config(secret_key: SecretKey, allowlist: IrohAllowlistConfig) -> IrohEndpointConfig {
    IrohEndpointConfig {
        secret_key,
        node_identity: None,
        relay_mode: IrohRelayModeConfig::Disabled,
        discovery: IrohDiscoveryConfig::Disabled,
        bind_port: None,
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        max_concurrent_multipath_paths: None,
        allowlist,
    }
}

async fn node(allowlist: IrohAllowlistConfig) -> (IrohTransport, Events, edge::Task<()>) {
    let key = SecretKey::generate();
    let (command_tx, events, _replicators, task) =
        spawn_endpoint(config(key.clone(), allowlist), edge::testing::edge())
            .await
            .unwrap();
    (IrohTransport::new(command_tx, key), events, task)
}

async fn connect(from: &IrohTransport, to: &IrohTransport) {
    from.dial(to.local_peer_id(), to.listen_addresses().await.unwrap())
        .await
        .unwrap();
    from.poll_until_connected(to.local_peer_id(), Duration::from_secs(5))
        .await
        .unwrap();
}

async fn wait_for_topic_peer(transport: &IrohTransport, peer: &PeerId) {
    timeout(Duration::from_secs(10), async {
        loop {
            if transport
                .topic_peers(DefraTopic::Encryption)
                .await
                .unwrap()
                .contains(peer)
            {
                return;
            }
            crate::rt::yield_now().await;
        }
    })
    .await
    .expect("the encryption topic did not form the line");
}

/// The first raw message on the encryption topic: who handed it over, who
/// published it, and its payload.
async fn next_request(events: &mut Events) -> (PeerId, PeerId, Vec<u8>) {
    loop {
        let event = timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("timed out waiting for the relayed request")
            .expect("iroh event channel closed");
        if let TransportEvent::GossipRawMessage {
            propagation_source,
            source,
            topic,
            data,
            ..
        } = event
        {
            if topic == ENCRYPTION_TOPIC {
                return (propagation_source, source, data);
            }
        }
    }
}

#[test]
fn a_relayed_raw_request_names_its_publisher() {
    workers::block_on(async {
        let (relay, _relay_events, relay_task) = node(IrohAllowlistConfig::AcceptAll).await;
        let only_relay = || {
            IrohAllowlistConfig::Explicit([relay.local_peer_id().to_string()].into_iter().collect())
        };
        let (asker, _asker_events, asker_task) = node(only_relay()).await;
        let (responder, mut responder_events, responder_task) = node(only_relay()).await;
        // The allowlist is inbound-only and gossip can hand each end the
        // other's address, so the ends also deny each other.
        asker.deny_peer(responder.local_peer_id()).await.unwrap();
        responder.deny_peer(asker.local_peer_id()).await.unwrap();
        connect(&asker, &relay).await;
        connect(&responder, &relay).await;

        for transport in [&asker, &relay, &responder] {
            transport
                .subscribe_raw(ENCRYPTION_TOPIC.to_string())
                .await
                .unwrap();
        }
        wait_for_topic_peer(&asker, relay.local_peer_id()).await;
        wait_for_topic_peer(&relay, asker.local_peer_id()).await;
        wait_for_topic_peer(&relay, responder.local_peer_id()).await;
        wait_for_topic_peer(&responder, relay.local_peer_id()).await;

        asker
            .publish_raw(ENCRYPTION_TOPIC.to_string(), b"request".to_vec())
            .await
            .unwrap();
        let (propagation_source, source, data) = next_request(&mut responder_events).await;

        assert_eq!(&propagation_source, relay.local_peer_id());
        assert_eq!(&source, asker.local_peer_id());
        assert_eq!(data, b"request");

        for transport in [asker, relay, responder] {
            transport.shutdown().await.unwrap();
        }
        for task in [asker_task, relay_task, responder_task] {
            task.await.unwrap();
        }
    })
    .unwrap();
}

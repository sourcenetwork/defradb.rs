//! Gossip rides the mux connection: it re-meshes when a peer comes back, and
//! stops when a peer is revoked.
//!
//! Run with:
//!   cargo test -p integration-test --test p2p_iroh -- connection::gossip_remesh

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use nuclei::time::timeout;
use p2p::iroh::{
    load_or_generate_secret_key, spawn_endpoint, IrohDiscoveryConfig, IrohEndpointConfig,
    IrohRelayModeConfig, IrohTransport,
};
use p2p::sync::DispatchReceiver;
use p2p::topics::{DefraTopic, ENCRYPTION_TOPIC};
use p2p::{P2PTransport, TransportEvent};

async fn test_config() -> IrohEndpointConfig {
    IrohEndpointConfig {
        secret_key: load_or_generate_secret_key(None)
            .await
            .expect("generate iroh key"),
        node_identity: None,
        relay_mode: IrohRelayModeConfig::Disabled,
        discovery: IrohDiscoveryConfig::Disabled,
        bind_port: None,
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        max_concurrent_multipath_paths: None,
        allowlist: Default::default(),
    }
}

async fn wait_peer_subscribed<T>(
    events: &mut DispatchReceiver<TransportEvent<T>>,
    want_topic: &str,
    context: &str,
) {
    loop {
        let event = timeout(Duration::from_secs(15), events.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for PeerSubscribed ({context})"))
            .expect("iroh event channel closed");
        if let TransportEvent::PeerSubscribed { topic, .. } = &event {
            if topic == want_topic {
                return;
            }
        }
    }
}

async fn wait_peer_unsubscribed<T>(
    events: &mut DispatchReceiver<TransportEvent<T>>,
    want_topic: &str,
    context: &str,
) {
    loop {
        let event = timeout(Duration::from_secs(30), events.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for PeerUnsubscribed ({context})"))
            .expect("iroh event channel closed");
        if let TransportEvent::PeerUnsubscribed { topic, .. } = &event {
            if topic == want_topic {
                return;
            }
        }
    }
}

async fn assert_raw_delivery<T>(
    from: &IrohTransport,
    to_events: &mut DispatchReceiver<TransportEvent<T>>,
    payload: Vec<u8>,
    context: &str,
) {
    from.publish_raw(ENCRYPTION_TOPIC.to_string(), payload.clone())
        .await
        .expect("publish_raw");
    loop {
        let event = timeout(Duration::from_secs(10), to_events.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for gossip delivery ({context})"))
            .expect("iroh event channel closed");
        if let TransportEvent::GossipRawMessage { topic, data, .. } = event {
            if topic == ENCRYPTION_TOPIC && data == payload {
                return;
            }
        }
    }
}

/// Kill one peer, restart it under the same identity, and the survivor's
/// gossip mesh comes back once the two reconnect (#1092).
#[nuclei::test]
async fn remesh_after_peer_restart() {
    edge::compat(async move {
        let config0 = test_config().await;
        let config1 = test_config().await;
        let key0 = config0.secret_key.clone();
        let key1 = config1.secret_key.clone();

        let (tx0, mut events0, _r0, task0) =
            spawn_endpoint(config0).await.expect("spawn endpoint 0");
        let (tx1, mut events1, _r1, task1) =
            spawn_endpoint(config1).await.expect("spawn endpoint 1");
        let transport0 = IrohTransport::new(tx0, key0);
        let transport1 = IrohTransport::new(tx1, key1.clone());

        transport0
            .dial(
                transport1.local_peer_id(),
                transport1.listen_addresses().await.expect("addrs 1"),
            )
            .await
            .expect("dial 0→1");
        transport0
            .poll_until_connected(transport1.local_peer_id(), Duration::from_secs(5))
            .await
            .expect("0 sees 1");
        transport1
            .poll_until_connected(transport0.local_peer_id(), Duration::from_secs(5))
            .await
            .expect("1 sees 0");

        transport0
            .subscribe(DefraTopic::Encryption)
            .await
            .expect("subscribe 0");
        transport1
            .subscribe(DefraTopic::Encryption)
            .await
            .expect("subscribe 1");
        transport0
            .register_pubsub_rpc_topic(ENCRYPTION_TOPIC.to_string())
            .await
            .expect("raw topic 0");
        wait_peer_subscribed(&mut events0, ENCRYPTION_TOPIC, "initial mesh, node 0").await;
        wait_peer_subscribed(&mut events1, ENCRYPTION_TOPIC, "initial mesh, node 1").await;

        // Kill node 1's transport entirely.
        transport1.shutdown().await.expect("shutdown 1");
        task1.join().await.expect("endpoint task 1");
        drop(transport1);
        wait_peer_unsubscribed(&mut events0, ENCRYPTION_TOPIC, "after node 1 death").await;

        // Bring node 1 back under the same identity (new ephemeral port).
        let mut config1b = test_config().await;
        config1b.secret_key = key1.clone();
        let (tx1b, mut events1b, _r1b, task1b) =
            spawn_endpoint(config1b).await.expect("respawn endpoint 1");
        let transport1b = IrohTransport::new(tx1b, key1);
        transport1b
            .subscribe(DefraTopic::Encryption)
            .await
            .expect("resubscribe 1");

        transport0
            .dial(
                transport1b.local_peer_id(),
                transport1b.listen_addresses().await.expect("addrs 1b"),
            )
            .await
            .expect("redial 0→1");
        transport0
            .poll_until_connected(transport1b.local_peer_id(), Duration::from_secs(5))
            .await
            .expect("0 sees 1 again");

        wait_peer_subscribed(
            &mut events0,
            ENCRYPTION_TOPIC,
            "mesh after reconnect, node 0",
        )
        .await;
        wait_peer_subscribed(
            &mut events1b,
            ENCRYPTION_TOPIC,
            "mesh after reconnect, node 1",
        )
        .await;

        assert_raw_delivery(
            &transport1b,
            &mut events0,
            vec![0xCB, 0x01, 0x02],
            "1→0 after reconnect",
        )
        .await;

        transport0.shutdown().await.expect("shutdown 0");
        transport1b.shutdown().await.expect("shutdown 1b");
        task0.join().await.expect("endpoint task 0");
        task1b.join().await.expect("endpoint task 1b");
    })
    .await;
}

/// Revoking a peer closes its connection, and gossip rode that connection:
/// nothing gossiped reaches the revoked peer afterwards, in either direction.
#[nuclei::test]
async fn a_revoked_peer_gets_no_more_gossip() {
    edge::compat(async move {
        let config0 = test_config().await;
        let config1 = test_config().await;
        let key0 = config0.secret_key.clone();
        let key1 = config1.secret_key.clone();
        let (tx0, mut events0, _r0, task0) =
            spawn_endpoint(config0).await.expect("spawn endpoint 0");
        let (tx1, mut events1, _r1, task1) =
            spawn_endpoint(config1).await.expect("spawn endpoint 1");
        let transport0 = IrohTransport::new(tx0, key0);
        let transport1 = IrohTransport::new(tx1, key1);

        for transport in [&transport0, &transport1] {
            transport
                .subscribe(DefraTopic::Encryption)
                .await
                .expect("subscribe");
            transport
                .register_pubsub_rpc_topic(ENCRYPTION_TOPIC.to_string())
                .await
                .expect("raw topic");
        }
        transport0
            .dial(
                transport1.local_peer_id(),
                transport1.listen_addresses().await.expect("addrs 1"),
            )
            .await
            .expect("dial 0→1");
        wait_peer_subscribed(&mut events0, ENCRYPTION_TOPIC, "mesh, node 0").await;
        wait_peer_subscribed(&mut events1, ENCRYPTION_TOPIC, "mesh, node 1").await;
        assert_raw_delivery(&transport1, &mut events0, vec![1], "1→0 before revoke").await;

        transport0
            .deny_peer(transport1.local_peer_id())
            .await
            .expect("revoke 1");
        wait_peer_unsubscribed(&mut events0, ENCRYPTION_TOPIC, "after revoke, node 0").await;

        for (from, to_events, context) in [
            (&transport1, &mut events0, "1→0 after revoke"),
            (&transport0, &mut events1, "0→1 after revoke"),
        ] {
            from.publish_raw(ENCRYPTION_TOPIC.to_string(), vec![2])
                .await
                .expect("publish_raw");
            let delivered = timeout(Duration::from_secs(3), async {
                loop {
                    match to_events.recv().await {
                        Some(TransportEvent::GossipRawMessage { data, .. }) if data == [2] => {
                            return true
                        }
                        Some(_) => continue,
                        None => return false,
                    }
                }
            })
            .await
            .unwrap_or(false);
            assert!(!delivered, "gossip reached a revoked peer ({context})");
        }

        transport0.shutdown().await.expect("shutdown 0");
        transport1.shutdown().await.expect("shutdown 1");
        task0.join().await.expect("endpoint task 0");
        task1.join().await.expect("endpoint task 1");
    })
    .await;
}

//! Inbound-connection allowlist tests.
//!
//! Enforcement lives in `handle_incoming`, before a peer's identity
//! (established from the accepted QUIC connection itself) ever reaches the
//! gossip layer or the mux layer.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use bytes::Bytes;
use iroh::SecretKey;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Instant};

use super::{
    spawn_endpoint, IrohAllowlistConfig, IrohDiscoveryConfig, IrohEndpointConfig, IrohTransport,
};
use crate::message::PushLogBroadcast;
use crate::topics::DefraTopic;
use crate::transport::{P2PTransport, TransportEvent};

type Events = Receiver<TransportEvent<iroh::endpoint::SendStream>>;

fn config(secret_key: SecretKey, allowlist: IrohAllowlistConfig) -> IrohEndpointConfig {
    IrohEndpointConfig {
        secret_key,
        node_identity: None,
        relay_mode: super::IrohRelayModeConfig::Disabled,
        discovery: IrohDiscoveryConfig::Disabled,
        bind_port: None,
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        max_concurrent_multipath_paths: None,
        gossip_heal: Default::default(),
        allowlist,
    }
}

async fn spawn_node(allowlist: IrohAllowlistConfig) -> (IrohTransport, Events, JoinHandle<()>) {
    let secret_key = SecretKey::generate();
    let (command_tx, events, _replicators, task) =
        spawn_endpoint(config(secret_key.clone(), allowlist))
            .await
            .unwrap();
    (IrohTransport::new(command_tx, secret_key), events, task)
}

fn test_broadcast() -> PushLogBroadcast {
    PushLogBroadcast::new(
        "doc".to_string(),
        Bytes::from_static(&[1, 2, 3]),
        "collection".to_string(),
        "creator".to_string(),
        Bytes::from_static(&[4, 5, 6]),
    )
}

/// Wait for `PeerSubscribed` on `topic`: proof the gossip mesh grafted a
/// neighbor before a publish is attempted, so the send is deliverable rather
/// than dropped on an empty topic.
async fn wait_peer_subscribed(events: &mut Events, topic: &str) {
    loop {
        let event = timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("timed out waiting for peer subscription")
            .expect("iroh event channel closed");
        if let TransportEvent::PeerSubscribed { topic: t, .. } = &event {
            if t == topic {
                return;
            }
        }
    }
}

/// Wait for a `GossipMessage` from `dialer`, ignoring any other event that
/// arrives first (a `PeerSubscribed` from a still-forming mesh, say).
async fn wait_gossip_message(events: &mut Events, dialer: &IrohTransport) {
    loop {
        let event = timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("timed out waiting for gossip message")
            .expect("iroh event channel closed");
        if let TransportEvent::GossipMessage {
            propagation_source, ..
        } = event
        {
            assert_eq!(propagation_source, *dialer.local_peer_id());
            return;
        }
    }
}

/// A peer refused by the allowlist must never appear connected on the
/// accepting side, for as long as we keep checking.
async fn assert_never_connects(dialer: &IrohTransport, server: &IrohTransport, window: Duration) {
    let deadline = tokio::time::Instant::now() + window;
    while tokio::time::Instant::now() < deadline {
        let connected = server.connected_peers().await.unwrap();
        assert!(
            !connected.iter().any(|p| p == dialer.local_peer_id()),
            "a peer refused by the allowlist must never appear connected on the accepting side"
        );
        tokio::task::yield_now().await;
    }
}

/// Dial `server` from `dialer` and wait until the dialer's own view shows the
/// connection came up. Proves the dial actually reached the server (the
/// dialer's QUIC handshake completes locally before the server's allowlist
/// check, running in the accept path, can close it), without depending on
/// whether the server ends up accepting or refusing it.
async fn dial_and_wait_for_local_handshake(dialer: &IrohTransport, server: &IrohTransport) {
    let addrs = server.listen_addresses().await.unwrap();
    let _ = dialer.dial(server.local_peer_id(), addrs).await;
    timeout(Duration::from_secs(5), async {
        loop {
            if dialer
                .connected_peers()
                .await
                .unwrap()
                .iter()
                .any(|p| p == server.local_peer_id())
            {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("dial never reached the server");
}

async fn shutdown_all(
    dialer: IrohTransport,
    server: IrohTransport,
    dialer_task: JoinHandle<()>,
    server_task: JoinHandle<()>,
) {
    dialer.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
    dialer_task.await.unwrap();
    server_task.await.unwrap();
}

/// How long a refused peer is given to deliver gossip before the test
/// concludes that it cannot. Long enough for the mesh to have healed and
/// retried several times if the allowlist were not holding.
const GOSSIP_OBSERVATION_WINDOW: Duration = Duration::from_secs(2);

/// A peer not on the allowlist is refused: it never appears connected on the
/// accepting side, and no gossip message crosses (the gossip ALPN connection
/// the mesh would open is refused by the same check).
#[tokio::test]
async fn refuses_a_peer_not_on_the_allowlist_and_blocks_gossip() {
    let (dialer, _dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    // An allowlist naming someone else only: the dialer is never in it.
    let unrelated = SecretKey::generate().public().to_string();
    let (server, mut server_events, server_task) = spawn_node(IrohAllowlistConfig::Explicit(
        [unrelated].into_iter().collect(),
    ))
    .await;

    dial_and_wait_for_local_handshake(&dialer, &server).await;
    assert_never_connects(&dialer, &server, Duration::from_millis(500)).await;

    let topic = DefraTopic::collection("collection");
    dialer.subscribe(topic.clone()).await.unwrap();
    server.subscribe(topic.clone()).await.unwrap();

    // Publish throughout the observation window and inspect every event the
    // server sees, rather than sampling the first one. One publish followed
    // by one `recv` passes the moment any unrelated event arrives first,
    // which proves nothing: what has to hold is that no `GossipMessage`
    // appears at all before the deadline.
    let deadline = Instant::now() + GOSSIP_OBSERVATION_WINDOW;
    while Instant::now() < deadline {
        dialer
            .publish(topic.clone(), test_broadcast())
            .await
            .unwrap();
        match timeout(Duration::from_millis(100), server_events.recv()).await {
            Ok(Some(event)) => assert!(
                !matches!(event, TransportEvent::GossipMessage { .. }),
                "a refused peer must not be able to deliver gossip: {event:?}"
            ),
            // The endpoint closed its event stream; there is nothing left to
            // observe and nothing was delivered.
            Ok(None) => break,
            // A quiet tick, which is the expected shape of this test: keep
            // publishing until the window is over.
            Err(_) => {}
        }
    }

    shutdown_all(dialer, server, dialer_task, server_task).await;
}

/// A peer on the allowlist connects and exchanges gossip exactly as it would
/// without one configured.
#[tokio::test]
async fn accepts_a_peer_on_the_allowlist_and_exchanges_gossip() {
    let (dialer, mut dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let (server, mut server_events, server_task) = spawn_node(IrohAllowlistConfig::Explicit(
        [dialer.local_peer_id().to_string()].into_iter().collect(),
    ))
    .await;

    dialer
        .dial(
            server.local_peer_id(),
            server.listen_addresses().await.unwrap(),
        )
        .await
        .unwrap();
    dialer
        .poll_until_connected(server.local_peer_id(), Duration::from_secs(5))
        .await
        .unwrap();
    server
        .poll_until_connected(dialer.local_peer_id(), Duration::from_secs(5))
        .await
        .unwrap();

    let topic = DefraTopic::collection("collection");
    dialer.subscribe(topic.clone()).await.unwrap();
    server.subscribe(topic.clone()).await.unwrap();
    wait_peer_subscribed(&mut dialer_events, &topic.to_string()).await;
    wait_peer_subscribed(&mut server_events, &topic.to_string()).await;

    dialer.publish(topic, test_broadcast()).await.unwrap();
    wait_gossip_message(&mut server_events, &dialer).await;

    shutdown_all(dialer, server, dialer_task, server_task).await;
}

/// Neither side names the other: relies entirely on `IrohAllowlistConfig`'s
/// `Default` (`AcceptAll`), matching the transport's behavior before this
/// allowlist existed.
#[tokio::test]
async fn default_allowlist_accepts_every_peer_like_before() {
    let (dialer, mut dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::default()).await;
    let (server, mut server_events, server_task) = spawn_node(IrohAllowlistConfig::default()).await;

    dialer
        .dial(
            server.local_peer_id(),
            server.listen_addresses().await.unwrap(),
        )
        .await
        .unwrap();
    dialer
        .poll_until_connected(server.local_peer_id(), Duration::from_secs(5))
        .await
        .unwrap();
    server
        .poll_until_connected(dialer.local_peer_id(), Duration::from_secs(5))
        .await
        .unwrap();

    let topic = DefraTopic::collection("collection");
    dialer.subscribe(topic.clone()).await.unwrap();
    server.subscribe(topic.clone()).await.unwrap();
    wait_peer_subscribed(&mut dialer_events, &topic.to_string()).await;
    wait_peer_subscribed(&mut server_events, &topic.to_string()).await;

    dialer.publish(topic, test_broadcast()).await.unwrap();
    wait_gossip_message(&mut server_events, &dialer).await;

    shutdown_all(dialer, server, dialer_task, server_task).await;
}

/// A peer added to the allowlist while the endpoint is running is accepted
/// on the very next dial, without a restart.
#[tokio::test]
async fn allow_peer_authorizes_a_peer_added_at_runtime() {
    let (dialer, mut dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let (server, mut server_events, server_task) =
        spawn_node(IrohAllowlistConfig::Explicit(Default::default())).await;

    // Refused before the runtime update.
    dial_and_wait_for_local_handshake(&dialer, &server).await;
    assert_never_connects(&dialer, &server, Duration::from_millis(300)).await;

    server.allow_peer(dialer.local_peer_id()).await.unwrap();

    // The refused attempt above was torn down by the server; redial now that
    // the peer is authorized.
    dialer
        .dial(
            server.local_peer_id(),
            server.listen_addresses().await.unwrap(),
        )
        .await
        .unwrap();
    dialer
        .poll_until_connected(server.local_peer_id(), Duration::from_secs(5))
        .await
        .unwrap();
    server
        .poll_until_connected(dialer.local_peer_id(), Duration::from_secs(5))
        .await
        .unwrap();

    let topic = DefraTopic::collection("collection");
    dialer.subscribe(topic.clone()).await.unwrap();
    server.subscribe(topic.clone()).await.unwrap();
    wait_peer_subscribed(&mut dialer_events, &topic.to_string()).await;
    wait_peer_subscribed(&mut server_events, &topic.to_string()).await;

    dialer.publish(topic, test_broadcast()).await.unwrap();
    wait_gossip_message(&mut server_events, &dialer).await;

    shutdown_all(dialer, server, dialer_task, server_task).await;
}

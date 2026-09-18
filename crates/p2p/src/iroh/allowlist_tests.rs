//! Inbound-connection allowlist tests.
//!
//! Enforcement lives in `handle_incoming`, before a peer's identity
//! (established from the accepted QUIC connection itself) ever reaches the
//! gossip layer or the mux layer.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use bytes::Bytes;
use iroh::SecretKey;
use n0_future::task::JoinHandle;
use n0_future::time::{timeout, Instant};
use tokio::sync::mpsc::Receiver;

use super::endpoint_config::AdmissionAuthority;
use super::{
    spawn_endpoint, IrohAllowlistConfig, IrohDiscoveryConfig, IrohEndpointConfig, IrohTransport,
};
use crate::message::PushLogBroadcast;
use crate::topics::DefraTopic;
use crate::transport::{P2PTransport, PeerId, TransportEvent};

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
    let deadline = n0_future::time::Instant::now() + window;
    while n0_future::time::Instant::now() < deadline {
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

/// Poll `transport`'s own view of `connected_peers` until it no longer names
/// `peer_id`, or panic once `window` has passed without that happening.
async fn poll_until_not_connected(transport: &IrohTransport, peer_id: &PeerId, window: Duration) {
    let deadline = tokio::time::Instant::now() + window;
    loop {
        let connected = transport.connected_peers().await.unwrap();
        if !connected.iter().any(|p| p == peer_id) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {peer_id} to disconnect"
        );
        tokio::task::yield_now().await;
    }
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

    server
        .allow_peer(dialer.local_peer_id(), AdmissionAuthority::full())
        .await
        .unwrap();

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

/// The guarantee that makes this a real revoke rather than a half-measure:
/// denying a peer that is currently connected does not just stop its NEXT
/// connection attempt, it also closes the connection it already holds.
///
/// Proven from both ends, not just by reading the allowlist set: the
/// denying side's own `connected_peers` drops the peer, AND the denied
/// peer's `connected_peers` empties too. The second half only happens if the
/// underlying QUIC connection was actually torn down; a set-only removal on
/// the server would leave the dialer still believing it is connected.
#[tokio::test]
async fn deny_peer_closes_an_already_open_connection() {
    let (dialer, _dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let dialer_id = dialer.local_peer_id().clone();
    let (server, _server_events, server_task) = spawn_node(IrohAllowlistConfig::Explicit(
        [dialer_id.to_string()].into_iter().collect(),
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
        .poll_until_connected(&dialer_id, Duration::from_secs(5))
        .await
        .unwrap();

    server.deny_peer(&dialer_id).await.unwrap();

    poll_until_not_connected(&server, &dialer_id, Duration::from_secs(5)).await;
    poll_until_not_connected(&dialer, server.local_peer_id(), Duration::from_secs(5)).await;

    shutdown_all(dialer, server, dialer_task, server_task).await;
}

/// Revoking under `AcceptAll` bars that one peer and nobody else.
///
/// This is why the bar is a set of its own rather than a narrowing of the
/// allowlist: an endpoint configured to accept everyone has no entry to
/// remove, but it must still be able to cut off a single peer without
/// turning into an explicit allowlist for every other peer on the network.
#[tokio::test]
async fn deny_peer_under_accept_all_bars_only_that_peer() {
    let (dialer, _dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let dialer_id = dialer.local_peer_id().clone();
    let (other, _other_events, other_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let (server, _server_events, server_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;

    server
        .deny_peer(&dialer_id)
        .await
        .expect("an AcceptAll endpoint must still be able to revoke one peer");

    // The revoked peer is refused.
    dial_and_wait_for_local_handshake(&dialer, &server).await;
    assert_never_connects(&dialer, &server, Duration::from_millis(300)).await;

    // Everyone else is untouched.
    other
        .dial(
            server.local_peer_id(),
            server.listen_addresses().await.unwrap(),
        )
        .await
        .unwrap();
    other
        .poll_until_connected(server.local_peer_id(), Duration::from_secs(5))
        .await
        .expect("revoking one peer must not narrow who else may connect");

    other.shutdown().await.unwrap();
    other_task.await.unwrap();
    shutdown_all(dialer, server, dialer_task, server_task).await;
}

/// The half that makes this a revocation rather than an allowlist withdrawal.
///
/// Barring only the inbound accept leaves the node free to dial the peer
/// itself, and it does: the replicator reconnect sweep dials exactly the
/// registered peers missing from `connected_peers`, and cutting a peer's
/// connection is precisely what marks it missing. So a peer revoked on the
/// accept path alone is re-dialled BY US within seconds and regains full
/// stream service over the connection we opened. This pins the outbound
/// refusal directly, without waiting on that sweep.
#[tokio::test]
async fn deny_peer_refuses_our_own_outbound_dial_to_the_revoked_peer() {
    let (dialer, _dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let (server, _server_events, server_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let server_id = server.local_peer_id().clone();

    // The revoking node here is `dialer`: it revokes `server`, then tries to
    // dial it. `server` itself would happily accept.
    dialer
        .deny_peer(&server_id)
        .await
        .expect("revoke the peer we are about to dial");

    // Assert on the REASON. `server` would accept this dial happily, so a bare
    // `is_err` risks passing on an unrelated failure; and two separate checks
    // enforce this (the pre-dial refusal and the post-registration re-check),
    // so naming the reason is what proves the refusal came from the
    // revocation rather than from the harness.
    let error = dialer
        .dial(&server_id, server.listen_addresses().await.unwrap())
        .await
        .expect_err("dialling a revoked peer must be refused");
    let message = error.to_string();
    assert!(
        message.contains("revoke"),
        "the dial must be refused BY the revocation; got: {message}"
    );

    // And nothing came up behind it.
    assert!(dialer
        .connected_peers()
        .await
        .unwrap()
        .iter()
        .all(|p| p != &server_id));

    shutdown_all(dialer, server, dialer_task, server_task).await;
}

/// `handle_dial` is not the only way this node opens an outbound connection.
/// Every RPC helper (identity resolution, doc/branchable sync, CAR fetch,
/// every fire-and-forget send) shares `connect_with_cache` instead of going
/// through `handle_dial`, and that path dials independently of the explicit
/// `Dial` command. A revoked peer must be refused there too, or ordinary RPC
/// traffic quietly re-opens exactly the connection `deny_peer` just closed.
#[tokio::test]
async fn a_revoked_peer_cannot_be_reached_by_an_outbound_rpc() {
    let (dialer, _dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let (server, _server_events, server_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let server_id = server.local_peer_id().clone();

    // The revoking node here is `dialer`: it revokes `server`, then reaches
    // for it over an ordinary RPC rather than the explicit `Dial` command.
    dialer
        .deny_peer(&server_id)
        .await
        .expect("revoke the peer we are about to reach over RPC");

    // Assert on the REASON, not merely on `is_err`. This node has no address
    // for `server` (it was never dialled), so the RPC fails either way and a
    // bare `is_err` passes just as happily with the admission check deleted:
    // verified by removing the check and watching this test still go green.
    // Only the refusal message distinguishes "we refused to dial a revoked
    // peer" from "we tried and could not reach it".
    let error = dialer
        .get_peer_identity(&server_id)
        .await
        .expect_err("an outbound RPC to a revoked peer must be refused");
    let message = error.to_string();
    assert!(
        message.contains("revoked"),
        "the RPC must be refused BY the revocation, not merely fail to connect; got: {message}"
    );

    // And nothing came up behind it.
    assert!(dialer
        .connected_peers()
        .await
        .unwrap()
        .iter()
        .all(|p| p != &server_id));

    shutdown_all(dialer, server, dialer_task, server_task).await;
}

/// A principal that cannot revoke a peer must not be able to un-revoke one.
///
/// Exercised through the real command round trip, not just
/// `PeerAdmission::allow` in isolation, so the authority genuinely survives
/// being carried across the channel into the endpoint's own state machine.
/// The peer must stay barred in BOTH directions after the refusal, because a
/// half-applied transition (allowlist widened, bar left in place) would
/// restore the peer the moment anyone lifted the bar for an unrelated reason.
#[tokio::test]
async fn a_caller_without_revoke_authority_cannot_undo_a_revocation() {
    let (dialer, _dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let dialer_id = dialer.local_peer_id().clone();
    let (server, _server_events, server_task) = spawn_node(IrohAllowlistConfig::Explicit(
        [dialer_id.to_string()].into_iter().collect(),
    ))
    .await;

    server.deny_peer(&dialer_id).await.expect("revoke the peer");

    let refused = server
        .allow_peer(&dialer_id, AdmissionAuthority::admit_only())
        .await;
    let message = refused
        .expect_err("a caller without revoke authority must not lift a revocation")
        .to_string();
    assert!(
        message.contains("revoke"),
        "the refusal must name the revocation as the reason; got: {message}"
    );

    // The peer is still barred: it cannot get in.
    dial_and_wait_for_local_handshake(&dialer, &server).await;
    assert_never_connects(&dialer, &server, Duration::from_millis(300)).await;

    // And the authority that CAN revoke can still restore it, proving the
    // refusal above left the state machine intact rather than wedged.
    server
        .allow_peer(&dialer_id, AdmissionAuthority::full())
        .await
        .expect("the authority that can revoke may restore");
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
        .expect("restoring with full authority must re-admit the peer");

    shutdown_all(dialer, server, dialer_task, server_task).await;
}

/// `is_peer_revoked` reports the bar, so a caller that created durable state
/// for a peer can find out it was revoked mid-flight and undo that state.
///
/// The adapter's `add_replicator` relies on this: registering a replicator is
/// several awaits long, and `deny_peer` bars the peer and then deletes its
/// replicator records, so a registration that started before the bar could
/// otherwise finish after the deletion and put the records back. Checking only
/// before starting is the wrong end of the race; this is the query that makes
/// the after-check possible.
#[tokio::test]
async fn is_peer_revoked_reports_the_bar_a_registration_has_to_undo() {
    let (dialer, _dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let (server, _server_events, server_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let server_id = server.local_peer_id().clone();

    assert!(
        !dialer
            .is_peer_revoked(&server_id)
            .await
            .expect("query an un-revoked peer"),
        "a peer nobody revoked must not report as revoked"
    );

    dialer.deny_peer(&server_id).await.expect("revoke the peer");

    assert!(
        dialer
            .is_peer_revoked(&server_id)
            .await
            .expect("query a revoked peer"),
        "a revoked peer must report as revoked, or a registration racing the \
         revoke can never learn it has to roll back"
    );

    // And lifting it with the authority that can clears the report again.
    dialer
        .allow_peer(&server_id, AdmissionAuthority::full())
        .await
        .expect("restore the peer");
    assert!(!dialer
        .is_peer_revoked(&server_id)
        .await
        .expect("query a restored peer"));

    shutdown_all(dialer, server, dialer_task, server_task).await;
}

/// A device may log back in: denying then re-allowing must restore the
/// ability to connect, exactly as `allow_peer_authorizes_a_peer_added_at_runtime`
/// proves for a peer that was never connected in the first place.
#[tokio::test]
async fn deny_peer_then_allow_peer_again_permits_reconnection() {
    let (dialer, mut dialer_events, dialer_task) = spawn_node(IrohAllowlistConfig::AcceptAll).await;
    let dialer_id = dialer.local_peer_id().clone();
    let (server, mut server_events, server_task) = spawn_node(IrohAllowlistConfig::Explicit(
        [dialer_id.to_string()].into_iter().collect(),
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
        .poll_until_connected(&dialer_id, Duration::from_secs(5))
        .await
        .unwrap();

    server.deny_peer(&dialer_id).await.unwrap();
    poll_until_not_connected(&server, &dialer_id, Duration::from_secs(5)).await;

    // Refused while denied.
    dial_and_wait_for_local_handshake(&dialer, &server).await;
    assert_never_connects(&dialer, &server, Duration::from_millis(300)).await;

    server
        .allow_peer(&dialer_id, AdmissionAuthority::full())
        .await
        .unwrap();

    // The refused attempt above was torn down by the server; redial now that
    // the peer is authorized again.
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
        .poll_until_connected(&dialer_id, Duration::from_secs(5))
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

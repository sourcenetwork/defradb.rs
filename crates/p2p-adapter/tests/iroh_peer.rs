//! The shared iroh peer's startup and shutdown contracts.
#![cfg(feature = "iroh")]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use defra_p2p_adapter::{IrohPeer, IrohPeerConfig};
use p2p::iroh::{IrohDiscoveryConfig, IrohEndpointConfig, IrohRelayModeConfig};
use storage::RegolithStore;

async fn start_peer(store: Arc<RegolithStore>, strict: bool) -> IrohPeer<RegolithStore> {
    start_peer_with_bus(store, strict, Arc::new(events::ChannelBus::new())).await
}

async fn start_peer_with_bus(
    store: Arc<RegolithStore>,
    strict: bool,
    event_bus: Arc<dyn events::Bus>,
) -> IrohPeer<RegolithStore> {
    let database = Arc::new(
        db::DB::open_from_arc(Arc::clone(&store))
            .await
            .expect("open database"),
    );
    let document_acp: Arc<dyn acp::DocumentACP> = Arc::new(acp::ZanzibarDocumentACP::new(
        Arc::new(acp::MemoryZanzibarStore::new()),
    ));
    let endpoint = IrohEndpointConfig {
        secret_key: p2p::iroh::load_or_generate_secret_key(None)
            .await
            .expect("secret key"),
        relay_mode: IrohRelayModeConfig::Disabled,
        discovery: IrohDiscoveryConfig::Disabled,
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        ..IrohEndpointConfig::default()
    };
    let mut config = IrohPeerConfig::new(endpoint, document_acp, edge::testing::edge());
    config.strict_replicated_doc_access = strict;
    IrohPeer::start(store, database, event_bus, config)
        .await
        .expect("start peer")
}

/// The endpoint accepts before `start` returns, so a block merged or served
/// before a caller could bind ACP would bypass it.
#[nuclei::test]
async fn document_acp_is_bound_before_start_returns() {
    let peer = start_peer(Arc::new(RegolithStore::in_memory().unwrap()), true).await;

    let merge_handler = &peer.replication.merge_handler;
    let bound = (
        merge_handler.document_acp().is_some(),
        merge_handler.strict_replicated_doc_access(),
        peer.coordinator.document_acp().is_some(),
    );
    peer.shutdown.shutdown().await;

    assert_eq!(
        bound,
        (true, true, true),
        "(merge ACP, strict, coordinator ACP) must be set when start returns"
    );
}

/// A task that outlives shutdown keeps the store open, so the same path
/// cannot be reopened (#1309).
#[nuclei::test]
async fn shutdown_releases_every_store_reference() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let peer = start_peer(Arc::clone(&store), false).await;

    peer.shutdown.shutdown().await;
    drop(peer);

    let released = p2p::rt::time::timeout(Duration::from_secs(5), async {
        while Arc::strong_count(&store) > 1 {
            p2p::rt::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        released.is_ok(),
        "{} store references outlived the peer",
        Arc::strong_count(&store) - 1
    );
}

async fn next_peer_connection(
    subscription: &mut events::Subscription,
) -> events::PeerConnectionData {
    p2p::rt::time::timeout(Duration::from_secs(10), subscription.recv())
        .await
        .expect("peer connection event within timeout")
        .expect("subscription open")
        .as_peer_connection()
        .expect("peer connection data")
        .clone()
}

/// Connecting and disconnecting a peer reach the event bus, so embedders can
/// track presence without polling `connected_peers` (#1989).
#[nuclei::test]
async fn peer_connection_changes_are_published() {
    let bus = Arc::new(events::ChannelBus::new());
    let mut connections =
        events::Bus::subscribe(bus.as_ref(), &[events::EventName::PeerConnection]);
    let local = start_peer_with_bus(
        Arc::new(RegolithStore::in_memory().unwrap()),
        false,
        bus.clone(),
    )
    .await;
    let remote = start_peer(Arc::new(RegolithStore::in_memory().unwrap()), false).await;

    let remote_addr = remote
        .ops
        .shareable_address()
        .await
        .expect("remote address")
        .expect("remote has a shareable address");
    local.ops.connect_peer(&remote_addr).await.expect("connect");

    let opened = next_peer_connection(&mut connections).await;
    assert_eq!(opened.peer_id, remote.local_peer_id);
    assert!(opened.connected);

    remote.shutdown.shutdown().await;
    let closed = next_peer_connection(&mut connections).await;
    assert_eq!(closed.peer_id, remote.local_peer_id);
    assert!(!closed.connected);

    local.shutdown.shutdown().await;
}

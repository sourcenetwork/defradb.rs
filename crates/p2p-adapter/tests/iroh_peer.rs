//! The shared iroh peer's startup and shutdown contracts.
#![cfg(feature = "iroh")]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use defra_p2p_adapter::{IrohPeer, IrohPeerConfig};
use p2p::iroh::{IrohDiscoveryConfig, IrohEndpointConfig, IrohRelayModeConfig};
use storage::RegolithStore;

async fn start_peer(store: Arc<RegolithStore>, strict: bool) -> IrohPeer<RegolithStore> {
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
    let mut config = IrohPeerConfig::new(endpoint, document_acp);
    config.strict_replicated_doc_access = strict;
    IrohPeer::start(store, database, Arc::new(events::ChannelBus::new()), config)
        .await
        .expect("start peer")
}

/// The endpoint accepts before `start` returns, so a block merged or served
/// before a caller could bind ACP would bypass it.
#[tokio::test]
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
#[tokio::test]
async fn shutdown_releases_every_store_reference() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let peer = start_peer(Arc::clone(&store), false).await;

    peer.shutdown.shutdown().await;
    drop(peer);

    let released = tokio::time::timeout(Duration::from_secs(5), async {
        while Arc::strong_count(&store) > 1 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        released.is_ok(),
        "{} store references outlived the peer",
        Arc::strong_count(&store) - 1
    );
}

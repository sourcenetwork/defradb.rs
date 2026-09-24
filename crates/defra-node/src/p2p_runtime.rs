//! Private P2P setup and lifecycle, over the shared iroh peer.
//!
//! Gated behind the `p2p` feature. Not part of the public `defra_node` API.

use std::sync::Arc;

use crate::P2PConfig;

type WireKmsCallback = Box<dyn FnOnce(Arc<dyn kms::KmsService>) + Send>;

/// The running peer's shutdown; see [`P2PLifecycle::shutdown`].
pub(super) struct P2PLifecycle {
    shutdown: defra_p2p_adapter::IrohPeerShutdown,
}

impl P2PLifecycle {
    pub(super) async fn shutdown(&self) {
        self.shutdown.shutdown().await;
    }
}

/// Internal result from P2P setup, carrying the type-erased ops and mutator.
pub(super) struct P2PSetupResult {
    pub(super) ops: Arc<dyn defra_http::P2POperations>,
    pub(super) lifecycle: Option<P2PLifecycle>,
    pub(super) mutator: Arc<dyn query::DocMutator>,
    pub(super) txn_broadcaster: Arc<dyn db::event::emission::TxnBroadcaster>,
    /// Type-erased KMS transport for this node's P2P system. lib.rs adds it
    /// to the DefraKms transports list and installs the serve handler.
    pub(super) kms_transport: Arc<dyn kms::KeyTransport>,
    /// This node's transport-level peer id (stringified). lib.rs binds it
    /// into the KMS so served ECIES replies carry the correct AAD peer id.
    pub(super) local_peer_id: String,
    /// Binds the KMS lib.rs builds once document ACP exists.
    pub(super) wire_kms: Option<WireKmsCallback>,
}

pub(super) async fn setup_p2p<S: storage::corekv::Store + 'static>(
    store: Arc<S>,
    database: Arc<db::DB<S>>,
    event_bus: Arc<dyn events::Bus>,
    config: &P2PConfig,
    node_identity: Option<Arc<identity::RawIdentity>>,
    document_acp: Arc<dyn acp::DocumentACP>,
    strict_replicated_doc_access: bool,
) -> anyhow::Result<P2PSetupResult> {
    let secret_key =
        p2p::iroh::load_or_generate_secret_key(config.secret_key_path.as_deref()).await?;
    let mut peer_config = defra_p2p_adapter::IrohPeerConfig::new(
        p2p::iroh::IrohEndpointConfig {
            secret_key,
            node_identity,
            relay_mode: config.relay_mode.clone(),
            discovery: config.discovery.clone(),
            bind_port: Some(config.port),
            bind_addr: config.bind_addr,
            max_concurrent_multipath_paths: config.max_concurrent_multipath_paths,
            gossip_heal: p2p::iroh::GossipHealConfig::from_env(),
            allowlist: config.allowlist.clone(),
        },
        document_acp,
    );
    peer_config.strict_replicated_doc_access = strict_replicated_doc_access;
    peer_config.sync = p2p::sync::SyncConfig {
        max_concurrent_dag_fetches: config.max_concurrent_dag_fetches,
        max_concurrent_push_tasks: config.max_concurrent_push_tasks,
        max_doc_sync_request_doc_ids: config.max_doc_sync_request_doc_ids,
        rate_limit_burst: config.rate_limit_burst,
        rate_limit_rate: config.rate_limit_rate,
        max_pending_dags: config.max_pending_dags,
        ..Default::default()
    };
    peer_config.rebroadcast_on_merge = config.rebroadcast_on_merge;
    peer_config.load_persisted_collections = config.load_persisted_collections;
    if !config.load_persisted_collections {
        tracing::info!(target: "defra_node", "skipping persisted P2P collection subscriptions");
    }

    let peer = defra_p2p_adapter::IrohPeer::start(store, database, event_bus, peer_config)
        .await
        .map_err(|error| anyhow::anyhow!("iroh peer failed to start: {error}"))?;
    tracing::info!(target: "defra_node", peer_id = %peer.local_peer_id, "P2P started (IROH/QUIC)");

    let merge_handler_for_kms = Arc::clone(&peer.replication.merge_handler_inner);
    Ok(P2PSetupResult {
        ops: Arc::clone(&peer.ops),
        lifecycle: Some(P2PLifecycle {
            shutdown: peer.shutdown.clone(),
        }),
        mutator: peer.replication.broadcast_mutator.clone(),
        txn_broadcaster: Arc::clone(&peer.replication.txn_broadcaster),
        kms_transport: peer.kms_transport.clone() as Arc<dyn kms::KeyTransport>,
        local_peer_id: peer.local_peer_id.clone(),
        wire_kms: Some(Box::new(move |kms| merge_handler_for_kms.set_kms(kms))),
    })
}

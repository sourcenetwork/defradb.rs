//! Private P2P setup over the iroh peer, and what both transports hand back.
//!
//! Not part of the public `defra_node` API.

use std::sync::Arc;

#[cfg(feature = "p2p")]
use crate::error::{Error, Result};
use crate::managed_p2p::ManagedP2PSystem;
#[cfg(feature = "p2p")]
use crate::managed_p2p::TransportKind;
use defra_p2p_adapter::ReplicatorPushOptions;
#[cfg(feature = "p2p")]
use defra_p2p_adapter::ReplicatorPushOptionsState;

pub(crate) type WireKmsCallback = Box<dyn FnOnce(Arc<dyn kms::KmsService>) + Send>;
#[cfg(feature = "libp2p")]
pub(crate) type WireDocumentAcpCallback = Box<dyn FnOnce(Arc<dyn acp::DocumentACP>, bool) + Send>;

pub(crate) struct P2PSetup {
    pub system: Arc<ManagedP2PSystem>,
    pub mutator: Arc<dyn query::DocMutator>,
    #[cfg(feature = "libp2p")]
    pub wire_document_acp: Option<WireDocumentAcpCallback>,
    /// Forwards committed `/tx` writes to P2P peers; mirrors what the CLI
    /// `P2PSetup` exposes. Without this, transactional writes commit locally
    /// but never replicate.
    pub txn_broadcaster: Arc<dyn db::event::emission::TxnBroadcaster>,
    /// Type-erased KMS transport for this node's P2P system. node.rs adds it
    /// to the DefraKms transports list and installs the serve handler.
    pub kms_transport: Arc<dyn kms::KeyTransport>,
    /// This node's transport-level peer id (stringified). node.rs binds it
    /// into the KMS so served ECIES replies carry the correct AAD peer id.
    pub local_peer_id: String,
    /// Defers wiring the late-built KMS into the inner merge handler. NAC
    /// isn't available when the P2P system is created, so the KMS is built
    /// later in node.rs.
    pub wire_kms: Option<WireKmsCallback>,
    /// SE remote query transport (owner-queries-replicator, #976). Lets this
    /// node act as an SE query OWNER, fanning `encrypted_<Collection>`
    /// queries to replicators. The SE key is read lazily because it's
    /// provisioned at runtime via `set_se_options`.
    pub se_transport: Option<Arc<dyn query::SeQueryTransport>>,
    /// Inbound management-channel serve deps, read lazily by the event loop and
    /// populated by node.rs once the controller (`P2POperations`) and NAC
    /// manager are built. The event loop drops manage requests until then.
    pub manage_hooks: defra_p2p_adapter::manage::hooks::ManageHooksCell,
    /// The `P2POperations` controller (the `adapter`) bound into `manage_hooks`
    /// after the NAC manager exists. node.rs uses it as `hooks.ops`.
    pub manage_controller: Arc<dyn defra_p2p_adapter::P2POperations>,
    /// Requester-side manage correlators (mutating + query). The event-loop
    /// clones deliver inbound replies; these clones are for the requester API
    /// (Task 6.3) and are bound into `manage_hooks` so requester and event loop
    /// agree on message_id correlation.
    pub manage_correlator: p2p::ManageCorrelator,
    pub manage_query_correlator: p2p::ManageQueryCorrelator,
}

/// Tee the SE key material from runtime `set_se_options` into the lazy handle
/// read by the owner/querier transport. Skips non-32-byte keys (#976).
pub(crate) fn tee_se_key(handle: &db::merge::SeKeyHandle, options: &ReplicatorPushOptions) {
    match &options.se_encryption_key {
        Some(key_bytes) => match <[u8; 32]>::try_from(key_bytes.as_slice()) {
            Ok(key) => db::merge::store_se_key(
                handle,
                Some(db::merge::SeKeyMaterial::new(
                    key,
                    options.se_identity_pubkey.clone(),
                )),
            ),
            Err(_) => {
                tracing::warn!(
                    len = key_bytes.len(),
                    "SE key from set_se_options is not 32 bytes; skipping owner-transport tee"
                );
            }
        },
        None => db::merge::store_se_key(handle, None),
    }
}

#[cfg(feature = "p2p")]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn setup_iroh<S>(
    store: Arc<S>,
    database: Arc<db::DB<S>>,
    event_bus: Arc<dyn events::Bus>,
    config: &crate::P2PConfig,
    node_identity: Option<Arc<identity::RawIdentity>>,
    document_acp: Arc<dyn acp::DocumentACP>,
    strict_replicated_doc_access: bool,
    peer_keyring: Option<&crate::PeerKeyring>,
    edge: &edge::Edge,
) -> Result<P2PSetup>
where
    S: storage::corekv::Store + 'static,
{
    use crate::config::PeerKeySource;
    use defra_p2p_adapter::{IrohPeer, IrohPeerConfig, TransportDocPusher};
    use storage::stores::Peerstore;

    let max_txn_retries = database.options().max_txn_retries();
    let retry_schedule = config.sync.retry_schedule()?;

    let secret_key = match config.peer_key {
        PeerKeySource::File => {
            p2p::iroh::load_or_generate_secret_key(config.secret_key_path.as_deref()).await?
        }
        PeerKeySource::Store => {
            let seed = crate::peer_key::load_with_keyring(
                &Peerstore::new(store.clone()).with_max_txn_retries(max_txn_retries),
                config.secret_key_path.as_deref(),
                peer_keyring,
            )
            .await?;
            p2p::iroh::SecretKey::from_bytes(&seed)
        }
    };
    let mut peer_config = IrohPeerConfig::new(
        p2p::iroh::IrohEndpointConfig {
            secret_key,
            node_identity,
            relay_mode: config.relay_mode.clone(),
            discovery: config.discovery.clone(),
            bind_port: config.port,
            bind_addr: config.bind_addr,
            max_concurrent_multipath_paths: config.max_concurrent_multipath_paths,
            allowlist: config.allowlist.clone(),
        },
        document_acp,
        edge.clone(),
    );
    peer_config.strict_replicated_doc_access = strict_replicated_doc_access;
    peer_config.sync = config.sync.sync_config();
    peer_config.retry_schedule = retry_schedule.clone();
    peer_config.rebroadcast_on_merge = config.rebroadcast_on_merge;
    peer_config.load_persisted_collections = config.load_persisted_collections;
    if !config.load_persisted_collections {
        tracing::info!(target: "defra_node", "skipping persisted P2P collection subscriptions");
    }
    let replicator_push_options = ReplicatorPushOptionsState::default();
    peer_config.replicator_push_options = Some(replicator_push_options.clone());
    let peer = IrohPeer::start(store.clone(), database.clone(), event_bus, peer_config)
        .await
        .map_err(Error::IrohStart)?;
    tracing::info!(target: "defra_node", peer_id = %peer.local_peer_id, "P2P started (IROH/QUIC)");

    // The SE key arrives at runtime through set_se_options, so the query
    // transport reads it lazily from a handle that callback fills.
    let se_key_handle = db::merge::empty_se_key_handle();
    let se_transport = Some(peer.se_query_transport(se_key_handle.clone()));
    let broadcast_mutator_for_se = peer.replication.broadcast_mutator.clone();
    let se_options_callback = Arc::new(move |options: ReplicatorPushOptions| {
        tee_se_key(&se_key_handle, &options);
        broadcast_mutator_for_se.set_se_options(db::merge::BroadcastSeOptions {
            encryption_key: options.se_encryption_key,
            identity_pubkey: options.se_identity_pubkey,
        })
    });
    let system = Arc::new(ManagedP2PSystem::with_replicator_push_options_callback(
        TransportKind::Iroh,
        Arc::clone(&peer.ops),
        crate::managed_p2p::ShutdownHandle::iroh(peer.shutdown.clone()),
        replicator_push_options,
        Some(se_options_callback),
    ));

    let retry_transport = peer.transport.clone();
    let retry_doc_pusher: Arc<dyn TransportDocPusher> = Arc::clone(&peer.doc_pusher);
    let retry_se_repusher: Arc<dyn db::merge::SeArtifactRepusher> =
        peer.replication.broadcast_mutator.clone();
    let retry_peers = Peerstore::new(store.clone())
        .with_max_txn_retries(max_txn_retries)
        .with_retry_schedule(retry_schedule)
        .placed_on(database.pool());
    system.set_retry_replicators(Arc::new(move || {
        let peerstore = retry_peers.clone();
        let transport = retry_transport.clone();
        let doc_pusher = retry_doc_pusher.clone();
        let se_repusher = retry_se_repusher.clone();
        Box::pin(async move {
            defra_p2p_adapter::run_retry_pass(
                &peerstore,
                &transport,
                &doc_pusher,
                Some(&se_repusher),
                true,
            )
            .await;
        })
    }));
    system.set_manage_requester(Arc::clone(&peer.manage.requester));

    let merge_handler_for_kms = Arc::clone(&peer.replication.merge_handler_inner);
    Ok(P2PSetup {
        system,
        mutator: peer.replication.broadcast_mutator.clone(),
        txn_broadcaster: Arc::clone(&peer.replication.txn_broadcaster),
        kms_transport: peer.kms_transport.clone() as Arc<dyn kms::KeyTransport>,
        local_peer_id: peer.local_peer_id.clone(),
        wire_kms: Some(Box::new(move |kms| merge_handler_for_kms.set_kms(kms))),
        #[cfg(feature = "libp2p")]
        wire_document_acp: None,
        se_transport,
        manage_hooks: peer.manage.hooks.clone(),
        manage_controller: Arc::clone(&peer.ops),
        manage_correlator: peer.manage.correlator.clone(),
        manage_query_correlator: peer.manage.query_correlator.clone(),
    })
}

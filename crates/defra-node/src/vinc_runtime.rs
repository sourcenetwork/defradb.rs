//! Private P2P setup over the vinc peer, as `p2p_runtime::setup_iroh` is over
//! the iroh peer.

use std::sync::Arc;

use defra_p2p_adapter::{ReplicatorPushOptions, ReplicatorPushOptionsState};

use crate::error::{Error, Result};
use crate::managed_p2p::{ManagedP2PSystem, ShutdownHandle, TransportKind};
use crate::p2p_runtime::{tee_se_key, P2PSetup};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn setup_vinc<S>(
    store: Arc<S>,
    database: Arc<db::DB<S>>,
    event_bus: Arc<dyn events::Bus>,
    config: &crate::VincConfig,
    node_identity: Option<Arc<identity::RawIdentity>>,
    document_acp: Arc<dyn acp::DocumentACP>,
    strict_replicated_doc_access: bool,
    peer_keyring: Option<&crate::PeerKeyring>,
    scope: edge::Scope,
) -> Result<P2PSetup>
where
    S: storage::corekv::Store + 'static,
{
    use crate::config::PeerKeySource;
    use defra_p2p_adapter::{TransportDocPusher, VincPeer, VincPeerConfig};
    use storage::stores::Peerstore;

    let max_txn_retries = database.options().max_txn_retries();
    let retry_schedule = config.sync.retry_schedule()?;

    // The seed the node's other transports use, so it keeps its peer id.
    let seed = match config.peer_key {
        PeerKeySource::File => zeroize::Zeroizing::new(
            p2p::iroh::load_or_generate_secret_key(config.secret_key_path.as_deref())
                .await?
                .to_bytes(),
        ),
        PeerKeySource::Store => {
            crate::peer_key::load_with_keyring(
                &Peerstore::new(store.clone()).with_max_txn_retries(max_txn_retries),
                config.secret_key_path.as_deref(),
                peer_keyring,
            )
            .await?
        }
    };
    let mut endpoint = p2p::vinc::VincEndpointConfig::new(seed, config.bind);
    endpoint.allowlist = config.allowlist.clone();
    endpoint.node_identity = node_identity;
    endpoint.max_connections = config.max_connections;
    endpoint.hubs = config.hubs.clone();
    let mut peer_config = VincPeerConfig::new(endpoint, document_acp, scope.edge().clone());
    peer_config.strict_replicated_doc_access = strict_replicated_doc_access;
    peer_config.sync = config.sync.sync_config();
    peer_config.retry_schedule = retry_schedule.clone();
    peer_config.load_persisted_collections = config.load_persisted_collections;
    let replicator_push_options = ReplicatorPushOptionsState::default();
    peer_config.replicator_push_options = Some(replicator_push_options.clone());
    let peer = VincPeer::start(store.clone(), database.clone(), event_bus, peer_config)
        .await
        .map_err(Error::VincStart)?;
    tracing::info!(target: "defra_node", peer_id = %peer.local_peer_id, "P2P started (vinc)");

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
        TransportKind::Vinc,
        Arc::clone(&peer.ops),
        ShutdownHandle::vinc(peer.shutdown.clone(), scope),
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
        kms_transport: Arc::clone(&peer.kms_transport),
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

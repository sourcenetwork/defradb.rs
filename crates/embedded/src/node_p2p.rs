use std::sync::Arc;

#[cfg(any(feature = "libp2p", feature = "iroh"))]
use crate::error::{Error, Result};
#[cfg(any(feature = "libp2p", feature = "iroh"))]
use p2p::sync::SyncConfig;
#[cfg(feature = "libp2p")]
use p2p::topics::DefraTopic;

use crate::node::WireKmsCallback;
#[cfg(feature = "libp2p")]
use crate::node::{EmbeddedBlockstore, WireDocumentAcpCallback};
#[cfg(feature = "libp2p")]
use crate::node_recovery::{restore_libp2p_documents, restore_libp2p_replicators};
#[cfg(feature = "libp2p")]
use crate::node_tasks::{spawn_libp2p_event_handler, spawn_node_task, spawn_replication_loop};
#[cfg(feature = "libp2p")]
use crate::Libp2pConfig;
use crate::ManagedP2PSystem;
#[cfg(any(feature = "libp2p", feature = "iroh"))]
use crate::TransportKind;
#[cfg(feature = "libp2p")]
use defra_p2p_adapter::{DbTransportDocPusher, DbVersionSyncer, P2PAdapter, TransportDocPusher};
#[cfg(any(feature = "libp2p", feature = "iroh"))]
use defra_p2p_adapter::{ReplicatorPushOptions, ReplicatorPushOptionsState};

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
    /// embedded node act as an SE query OWNER, fanning `encrypted_<Collection>`
    /// queries to replicators. The SE key is read lazily because it's
    /// provisioned at runtime via `set_se_options`.
    pub se_transport: Option<Arc<dyn query::SeQueryTransport>>,
    /// Inbound management-channel serve deps, read lazily by the event loop and
    /// populated by node.rs once the controller (`P2POperations`) and NAC
    /// manager are built. The event loop drops manage requests until then.
    pub manage_hooks: defra_p2p_adapter::manage::hooks::ManageHooksCell,
    /// The `P2POperations` controller (the `adapter`) bound into `manage_hooks`
    /// after the NAC manager exists. node.rs uses it as `hooks.ops`.
    pub manage_controller: Arc<dyn defra_http::P2POperations>,
    /// Requester-side manage correlators (mutating + query). The event-loop
    /// clones deliver inbound replies; these clones are for the requester API
    /// (Task 6.3) and are bound into `manage_hooks` so requester and event loop
    /// agree on message_id correlation.
    pub manage_correlator: p2p::ManageCorrelator,
    pub manage_query_correlator: p2p::ManageQueryCorrelator,
}

#[cfg(feature = "libp2p")]
async fn shutdown_libp2p_host(handle: &p2p::P2PHostHandle, host_task: edge::Service) {
    if let Err(error) = handle.shutdown().await {
        tracing::debug!(%error, "failed to signal P2P host during setup rollback");
    }
    host_task.stopped().await;
}

/// Builds the libp2p host and every task around it on `edge`: the p2p crate
/// spawns onto the runtime it runs on, so the caller may be on any thread.
#[cfg(feature = "libp2p")]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn setup_libp2p<S>(
    store: Arc<S>,
    database: Arc<db::DB<S>>,
    event_bus: Arc<dyn events::Bus>,
    config: &Libp2pConfig,
    sync_config: SyncConfig,
    retry_schedule: storage::stores::RetrySchedule,
    peer_keyring: Option<&crate::PeerKeyring>,
    edge: &edge::Edge,
) -> Result<P2PSetup>
where
    S: storage::corekv::Store + 'static,
{
    edge.spawn(assemble_libp2p(
        store,
        database,
        event_bus,
        config.clone(),
        sync_config,
        retry_schedule,
        peer_keyring.cloned(),
        edge.clone(),
    ))
    .await
    .map_err(Error::NetworkSetup)?
}

#[cfg(feature = "libp2p")]
#[allow(clippy::too_many_arguments)]
async fn assemble_libp2p<S>(
    store: Arc<S>,
    database: Arc<db::DB<S>>,
    event_bus: Arc<dyn events::Bus>,
    config: Libp2pConfig,
    sync_config: SyncConfig,
    retry_schedule: storage::stores::RetrySchedule,
    peer_keyring: Option<crate::PeerKeyring>,
    edge: edge::Edge,
) -> Result<P2PSetup>
where
    S: storage::corekv::Store + 'static,
{
    let edge = &edge;
    let peer_keyring = peer_keyring.as_ref();
    use p2p::bitswap::BitswapStoreAdapter;
    use p2p::sync::DocumentHeadProvider;
    use storage::stores::Peerstore;

    let max_txn_retries = database.options().max_txn_retries();

    let listen_addrs = config
        .listen_addrs
        .iter()
        .map(|addr| {
            addr.parse().map_err(|source| Error::InvalidMultiaddr {
                addr: addr.clone(),
                source,
            })
        })
        .collect::<std::result::Result<Vec<_>, Error>>()?;
    let blockstore = Arc::new(
        EmbeddedBlockstore::new(store.clone(), true)
            .with_max_txn_retries(max_txn_retries)
            .placed_on(database.pool()),
    );
    let bitswap_store = BitswapStoreAdapter::new(blockstore.clone());

    let mut seed = crate::node_peer_key::load_with_keyring(
        &Peerstore::new(store.clone()).with_max_txn_retries(max_txn_retries),
        None,
        peer_keyring,
    )
    .await?;
    let p2p_keypair =
        libp2p::identity::Keypair::ed25519_from_bytes(&mut *seed).map_err(Error::InvalidPeerKey)?;

    let classifier = defra_p2p_adapter::DbBlockClassifier::new_arc(database.clone());
    let serve_acp = Arc::new(p2p::bitswap::LateBoundServeAcp::new());
    let (host, handle, event_rx, replicator_registry) =
        p2p::P2PHost::with_keypair_and_config_and_identity_and_serve_gate(
            p2p_keypair,
            bitswap_store,
            p2p::P2PHostConfig {
                enable_pubsub: config.enable_pubsub,
                enable_relay: config.enable_relay,
                stream_timeout: config
                    .stream_timeout_secs
                    .unwrap_or(p2p::P2PHostConfig::default().stream_timeout),
                ..Default::default()
            },
            database.node_identity(),
            classifier.clone(),
            serve_acp.clone(),
            edge.clone(),
        )
        .await
        .map_err(Error::HostCreate)?;
    let host_task = spawn_node_task(edge, async move {
        host.run().await;
    });

    for listen_addr in listen_addrs {
        if let Err(error) = handle.listen(listen_addr).await {
            shutdown_libp2p_host(&handle, host_task).await;
            return Err(Error::Listen(error));
        }
    }

    for topic in [
        DefraTopic::DocSync,
        DefraTopic::Encryption,
        DefraTopic::Custom("sync-branchable".to_string()),
    ] {
        if let Err(error) = handle.subscribe(topic.clone()).await {
            tracing::warn!(topic = %topic, error = %error, "failed to subscribe to default topic");
        }
    }

    let collection_store = p2p::sync::pooled_collection_store(
        Arc::new(
            p2p::sync::P2PCollectionStore::new(store.clone()).with_max_txn_retries(max_txn_retries),
        ),
        database.pool(),
    );
    let head_provider: Arc<dyn DocumentHeadProvider> =
        Arc::new(db::merge::create_head_provider(database.clone()));
    let (mut coordinator, sync_events_rx) =
        match p2p::sync::SyncCoordinator::with_head_provider_and_serve_gate(
            p2p::Libp2pTransport::new(handle.clone()),
            blockstore.clone(),
            p2p::network::Network::on(edge.clone()),
            sync_config,
            p2p::bitswap::AccessMode::Controlled,
            replicator_registry,
            collection_store,
            head_provider,
            std::sync::Arc::new(replication_filter::QueryReplicationFilterMatcher::new()),
            classifier,
            serve_acp.clone(),
        )
        .await
        {
            Ok(coordinator) => coordinator,
            Err(error) => {
                shutdown_libp2p_host(&handle, host_task).await;
                return Err(Error::SyncCoordinator(error));
            }
        };

    let failure_rx = db::merge::attach_failure_channel(&mut coordinator, 1024);
    let coordinator = Arc::new(coordinator);
    // Its durable registrations change on the database's worker pool.
    coordinator
        .install_pending_dag_store(p2p::sync::pooled_pending_store(
            Arc::new(
                p2p::sync::PendingDagStore::new(store.clone())
                    .with_max_txn_retries(max_txn_retries),
            ),
            database.pool(),
        ))
        .await;
    let replication = db::merge::create_replication_stack(
        database.clone(),
        blockstore.clone(),
        coordinator.clone(),
    );

    let kms_libp2p_transport = p2p::Libp2pTransport::new(handle.clone());
    let local_peer_id = {
        use p2p::transport::P2PTransport;
        kms_libp2p_transport.local_peer_id().to_string()
    };
    let kms_transport = match p2p::kms::PubsubKeyTransport::new(
        kms_libp2p_transport,
        Arc::new(p2p::HandlePeerIdentityResolver::new(handle.clone())),
        edge.clone(),
    )
    .await
    {
        Ok(transport) => transport,
        Err(error) => {
            coordinator.shutdown().await;
            shutdown_libp2p_host(&handle, host_task).await;
            return Err(Error::KmsTransport(error));
        }
    };
    coordinator.install_kms_transport(kms_transport.clone());
    let merge_handler_inner_for_kms = replication.merge_handler_inner.clone();

    let coordinator_for_restore = coordinator.clone();
    let pending_dag_resync_task = spawn_node_task(edge, async move {
        coordinator_for_restore
            .run_pending_dag_resync(std::time::Duration::from_secs(60))
            .await;
    });

    // Receiver's re-arm loop (#1116 stage 2): dispatches due pending roots
    // at a tight cadence. Sibling of the resync sweep above.
    let coordinator_for_retry_clock = coordinator.clone();
    let pending_dag_retry_task = spawn_node_task(edge, async move {
        coordinator_for_retry_clock
            .run_pending_dag_retry_clock(std::time::Duration::from_secs(2))
            .await
            .expect("retry interval is nonzero");
    });

    // The fallback for a deferred verdict no arrival can release: one named
    // nothing, one past the index's capacity, and everything the index held
    // before this process started.
    // It runs on the database's worker pool and ends with the coordinator's
    // shutdown; the network task here only follows it for the node's teardown.
    let governance_sweep = db::merge::governance::start_governance_sweep(
        replication.merge_handler_inner.clone(),
        db::merge::governance::SWEEP_INTERVAL,
        coordinator.shutdown_handle(),
    );
    let governance_sweep_task = spawn_node_task(edge, async move {
        let _ = governance_sweep.await;
    });

    match db::merge::load_persisted_collections(&coordinator).await {
        Ok(count) if count > 0 => tracing::debug!(count, "loaded persisted P2P collections"),
        Ok(_) => {}
        Err(error) => tracing::warn!(error = %error, "failed to load persisted P2P collections"),
    }

    // Start pubsub_rpc doc-sync / sync-branchable services (#828) so this
    // node can interoperate with Go DefraDB peers over gossipsub.
    if let Err(error) = coordinator.start_pubsub_services().await {
        tracing::warn!(error = %error, "failed to start pubsub_rpc services");
    }

    // SE query correlator: lets this node serve as an SE replicator and route
    // any inbound replies. Cloned so the SAME correlator is shared between the
    // event handler (which delivers replies) and the owner/querier transport
    // (which awaits them) — they must agree on message_id correlation (#976).
    let se_correlator = p2p::SeQueryCorrelator::new();
    let se_correlator_for_transport = se_correlator.clone();
    // Manage channel: correlators shared between the event loop (which delivers
    // inbound replies) and the requester API (Task 6.3), and a deferred hooks
    // cell node.rs populates once the controller + NAC manager exist.
    let manage_correlator = p2p::ManageCorrelator::new();
    let manage_query_correlator = p2p::ManageQueryCorrelator::new();
    let manage_hooks = defra_p2p_adapter::manage::hooks::new_manage_hooks_cell();
    // The sender's retry state: its calls from the edge run on the pool.
    let retry_peers = Peerstore::new(store.clone())
        .with_max_txn_retries(max_txn_retries)
        .with_retry_schedule(retry_schedule.clone())
        .placed_on(database.pool());
    let host_event_task = spawn_libp2p_event_handler(
        event_rx,
        coordinator.clone(),
        store.clone(),
        retry_peers.clone(),
        event_bus.clone(),
        handle.clone(),
        se_correlator,
        manage_hooks.clone(),
        edge,
    );
    let replication_task = spawn_replication_loop(
        coordinator.clone(),
        sync_events_rx,
        replication.merge_handler_on_pool(),
        event_bus.clone(),
        edge,
    );
    let failure_recorder_task =
        defra_p2p_adapter::spawn_failure_recorder(retry_peers.clone(), failure_rx);

    let doc_pusher_impl = Arc::new(
        DbTransportDocPusher::new(
            database.clone(),
            p2p::Libp2pTransport::new(handle.clone()),
            coordinator.head_hint_car_authority(),
        )
        .with_retry_schedule(retry_schedule),
    );
    let doc_pusher_for_acp = doc_pusher_impl.clone();
    // Its storage calls run on the database's worker pool.
    let doc_pusher: Arc<dyn TransportDocPusher> =
        defra_p2p_adapter::pooled(doc_pusher_impl, database.pool());
    let version_syncer = Some(DbVersionSyncer::new_arc(
        blockstore.clone(),
        replication.merge_handler_inner.clone(),
        database.clone(),
    ));
    let se_repusher =
        db::merge::pooled_se_repusher(replication.broadcast_mutator.clone(), database.pool());
    replication
        .merge_handler_inner
        .set_se_repusher(se_repusher.clone());
    let retry_transport = p2p::Libp2pTransport::new(handle.clone());
    let retry_doc_pusher = doc_pusher.clone();
    let retry_se_repusher = se_repusher.clone();
    let retry_loop_task = defra_p2p_adapter::spawn_retry_loop(
        retry_peers.clone(),
        p2p::Libp2pTransport::new(handle.clone()),
        doc_pusher.clone(),
        Some(se_repusher),
        edge,
    );

    let restore_peerstore = Peerstore::new(store.clone()).with_max_txn_retries(max_txn_retries);
    restore_libp2p_replicators(&handle, &restore_peerstore).await;
    let restored_doc_ids = restore_libp2p_documents(&handle, &restore_peerstore).await;

    let replicator_push_options = ReplicatorPushOptionsState::default();
    let adapter = P2PAdapter::with_full_context(
        handle.clone(),
        coordinator.clone(),
        doc_pusher,
        event_bus,
        version_syncer,
        db::node_access_checker(database.clone()),
    )
    .with_replicator_push_options_state(replicator_push_options.clone());
    adapter.set_initial_tracked_documents(restored_doc_ids);
    let coordinator_for_acp = coordinator.clone();
    let serve_acp_for_acp = serve_acp.clone();
    let handle_for_acp = handle.clone();
    let broadcast_mutator_for_acp = replication.broadcast_mutator.clone();
    let merge_handler_for_acp = replication.merge_handler.clone();
    let broadcast_mutator_for_se = replication.broadcast_mutator.clone();
    // Lazy SE-key handle: teed by the callback below (runtime provisioning),
    // read by the owner/querier transport at query time (#976).
    let se_key_handle = db::merge::empty_se_key_handle();
    let se_key_handle_for_callback = se_key_handle.clone();
    let se_options_callback = Arc::new(move |options: ReplicatorPushOptions| {
        tee_se_key(&se_key_handle_for_callback, &options);
        broadcast_mutator_for_se.set_se_options(db::merge::BroadcastSeOptions {
            encryption_key: options.se_encryption_key,
            identity_pubkey: options.se_identity_pubkey,
        })
    });
    let se_transport: Option<Arc<dyn query::SeQueryTransport>> =
        Some(Arc::new(db::merge::DbMergeSeQueryTransport::new(
            p2p::Libp2pTransport::new(handle.clone()),
            se_correlator_for_transport,
            coordinator.replicators().clone(),
            se_key_handle,
            coordinator.network(),
        )) as Arc<dyn query::SeQueryTransport>);
    let manage_controller: Arc<dyn defra_http::P2POperations> = Arc::new(adapter);
    let system = Arc::new(ManagedP2PSystem::with_replicator_push_options_callback(
        TransportKind::Libp2p,
        manage_controller.clone(),
        crate::node::ShutdownHandle::libp2p(
            handle.clone(),
            coordinator.shutdown_handle(),
            vec![
                host_task,
                host_event_task,
                replication_task,
                pending_dag_resync_task,
                pending_dag_retry_task,
                governance_sweep_task,
            ],
            retry_loop_task,
            failure_recorder_task,
        ),
        replicator_push_options,
        Some(se_options_callback),
    ));
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

    // Outbound management requester over the same libp2p transport, sharing the
    // requester-side manage correlators (Task 7a). Installed on the system so an
    // HTTP consumer can wire it into `AppState` via `with_manage`.
    system.set_manage_requester(Arc::new(
        defra_p2p_adapter::manage::client::ManageClient::new(
            p2p::Libp2pTransport::new(handle.clone()),
            manage_correlator.clone(),
            manage_query_correlator.clone(),
        ),
    ));

    Ok(P2PSetup {
        system,
        mutator: replication.broadcast_mutator,
        txn_broadcaster: replication.txn_broadcaster,
        kms_transport: kms_transport as Arc<dyn kms::KeyTransport>,
        local_peer_id,
        wire_kms: Some(Box::new(move |kms| {
            merge_handler_inner_for_kms.set_kms(kms);
        })),
        wire_document_acp: Some(Box::new(move |acp, strict| {
            serve_acp_for_acp.set(p2p::bitswap::ServeAcp {
                resolver: Arc::new(p2p::HandlePeerIdentityResolver::new(handle_for_acp)),
                gate: defra_p2p_adapter::DbBlockReadGate::new_arc(acp.clone()),
            });
            coordinator_for_acp.set_document_acp(acp.clone());
            doc_pusher_for_acp.set_document_acp(acp.clone());
            broadcast_mutator_for_acp.set_document_acp(acp.clone());
            merge_handler_for_acp.set_document_acp(acp);
            merge_handler_for_acp.set_strict_replicated_doc_access(strict);
        })),
        se_transport,
        manage_hooks,
        manage_controller,
        manage_correlator,
        manage_query_correlator,
    })
}

/// Tee the SE key material from runtime `set_se_options` into the lazy handle
/// read by the owner/querier transport. Skips non-32-byte keys (#976).
#[cfg(any(feature = "libp2p", feature = "iroh"))]
fn tee_se_key(handle: &db::merge::SeKeyHandle, options: &ReplicatorPushOptions) {
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

#[cfg(feature = "iroh")]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn setup_iroh<S>(
    store: Arc<S>,
    database: Arc<db::DB<S>>,
    event_bus: Arc<dyn events::Bus>,
    config: &crate::IrohConfig,
    sync_config: SyncConfig,
    retry_schedule: storage::stores::RetrySchedule,
    node_identity: Option<Arc<identity::RawIdentity>>,
    document_acp: Arc<dyn acp::DocumentACP>,
    strict_replicated_doc_access: bool,
    peer_keyring: Option<&crate::PeerKeyring>,
    edge: &edge::Edge,
) -> Result<P2PSetup>
where
    S: storage::corekv::Store + 'static,
{
    use defra_p2p_adapter::{IrohPeer, IrohPeerConfig, TransportDocPusher};
    use storage::stores::Peerstore;

    let max_txn_retries = database.options().max_txn_retries();

    let seed = crate::node_peer_key::load_with_keyring(
        &Peerstore::new(store.clone()).with_max_txn_retries(max_txn_retries),
        config.secret_key_path.as_deref(),
        peer_keyring,
    )
    .await?;
    let secret_key = p2p::iroh::SecretKey::from_bytes(&seed);
    let mut peer_config = IrohPeerConfig::new(
        p2p::iroh::IrohEndpointConfig {
            secret_key,
            node_identity,
            relay_mode: config.relay_mode.clone(),
            discovery: config.discovery.clone(),
            bind_port: config.bind_port,
            bind_addr: config.bind_addr,
            max_concurrent_multipath_paths: config.max_concurrent_multipath_paths,
            allowlist: config.allowlist.clone(),
        },
        document_acp,
        edge.clone(),
    );
    peer_config.strict_replicated_doc_access = strict_replicated_doc_access;
    peer_config.sync = sync_config;
    peer_config.retry_schedule = retry_schedule.clone();
    let replicator_push_options = ReplicatorPushOptionsState::default();
    peer_config.replicator_push_options = Some(replicator_push_options.clone());
    let peer = IrohPeer::start(store.clone(), database.clone(), event_bus, peer_config)
        .await
        .map_err(Error::IrohStart)?;

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
        crate::node::ShutdownHandle::iroh(peer.shutdown.clone()),
        replicator_push_options,
        Some(se_options_callback),
    ));

    let retry_transport = peer.transport.clone();
    let retry_doc_pusher: Arc<dyn TransportDocPusher> = Arc::clone(&peer.doc_pusher);
    let retry_se_repusher =
        db::merge::pooled_se_repusher(peer.replication.broadcast_mutator.clone(), database.pool());
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

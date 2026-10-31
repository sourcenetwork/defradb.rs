use std::sync::Arc;

use crate::config::Libp2pConfig;
use crate::error::{Error, Result};
use crate::libp2p_recovery::{restore_libp2p_documents, restore_libp2p_replicators};
use crate::libp2p_tasks::{spawn_libp2p_event_handler, spawn_node_task, spawn_replication_loop};
use crate::managed_p2p::{ManagedP2PSystem, ShutdownHandle, TransportKind};
use crate::p2p_runtime::{tee_se_key, P2PSetup};
use defra_p2p_adapter::{DbTransportDocPusher, DbVersionSyncer, P2PAdapter, TransportDocPusher};
use defra_p2p_adapter::{ReplicatorPushOptions, ReplicatorPushOptionsState};
use p2p::sync::SyncConfig;
use p2p::topics::DefraTopic;

pub(crate) type NodeBlockstore<S> = blockstore::DefraBlockstore<S>;
pub(crate) type NodeMergeHandler<S> = db::merge::AcpMergeHandler<S, NodeBlockstore<S>>;

async fn shutdown_libp2p_host(handle: &p2p::P2PHostHandle, host_task: edge::Service) {
    if let Err(error) = handle.shutdown().await {
        tracing::debug!(%error, "failed to signal P2P host during setup rollback");
    }
    host_task.stopped().await;
}

/// Builds the libp2p host and every task around it on `edge`: the p2p crate
/// spawns onto the runtime it runs on, so the caller may be on any thread.
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
        NodeBlockstore::new(store.clone(), true)
            .with_max_txn_retries(max_txn_retries)
            .placed_on(database.pool()),
    );
    let bitswap_store = BitswapStoreAdapter::new(blockstore.clone());

    let mut seed = crate::peer_key::load_with_keyring(
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
                shinzo_wire: config.shinzo_wire,
                stream_timeout: config
                    .stream_timeout_secs
                    .unwrap_or(p2p::P2PHostConfig::default().stream_timeout),
                ..Default::default()
            },
            database.node_identity(),
            classifier.clone(),
            serve_acp.clone(),
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

    let collection_store: Arc<dyn p2p::sync::P2PCollectionStorage> = Arc::new(
        p2p::sync::P2PCollectionStore::new(store.clone()).with_max_txn_retries(max_txn_retries),
    );
    let head_provider: Arc<dyn DocumentHeadProvider> =
        Arc::new(db::merge::create_head_provider(database.clone()));
    let (mut coordinator, sync_events_rx) =
        match p2p::sync::SyncCoordinator::with_head_provider_and_serve_gate(
            p2p::Libp2pTransport::new(handle.clone()),
            blockstore.clone(),
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
    // Its network work runs on the edge, also when a write on a pool worker
    // hands it a broadcast.
    coordinator.use_network(p2p::network::Network::on(edge.clone()));
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
    let se_repusher: Arc<dyn db::merge::SeArtifactRepusher> = replication.broadcast_mutator.clone();
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
    let manage_controller: Arc<dyn defra_p2p_adapter::P2POperations> = Arc::new(adapter);
    let system = Arc::new(ManagedP2PSystem::with_replicator_push_options_callback(
        TransportKind::Libp2p,
        manage_controller.clone(),
        ShutdownHandle::libp2p(
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

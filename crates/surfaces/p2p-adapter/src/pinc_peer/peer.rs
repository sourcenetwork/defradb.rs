//! Assembling the peer.

use std::sync::Arc;

use defra_pinc::PincTransport;
use p2p::bitswap::{LateBoundServeAcp, ServeAcp};
use p2p::P2PTransport;
use storage::corekv::Store;

use super::config::PincPeerConfig;
use super::events::spawn_event_handler;
use super::shutdown::PincPeerShutdown;
use crate::manage::hooks::ManageHooksCell;
use crate::{
    DbBlockClassifier, DbBlockReadGate, DbTransportDocPusher, DbTransportVersionSyncer,
    P2POperations, PincP2PAdapter, TransportDocPusher,
};

pub type PincBlockstore<S> = blockstore::DefraBlockstore<S>;
pub type PincCoordinator<S> = defra_pinc::PincSyncCoordinator<PincBlockstore<S>>;
pub type PincReplicationStack<S> = db::merge::ReplicationStack<S, PincBlockstore<S>, PincTransport>;

/// The management channel: inbound requests are served once `hooks` is
/// populated, and `requester` relays outbound ones.
pub struct PincManageChannel {
    pub hooks: ManageHooksCell,
    pub correlator: p2p::ManageCorrelator,
    pub query_correlator: p2p::ManageQueryCorrelator,
    pub requester: Arc<dyn crate::ManageRequester>,
}

/// A running pinc peer. Stop it with [`PincPeerShutdown::shutdown`].
pub struct PincPeer<S: Store + 'static> {
    pub transport: PincTransport,
    pub coordinator: Arc<PincCoordinator<S>>,
    pub replication: PincReplicationStack<S>,
    pub doc_pusher: Arc<dyn TransportDocPusher>,
    pub ops: Arc<dyn P2POperations>,
    pub manage: PincManageChannel,
    /// KMS key exchange over the raw `encryption` topics.
    #[cfg(feature = "kms")]
    pub kms_transport: Arc<p2p::kms::PubsubKeyTransport<PincTransport>>,
    pub local_peer_id: String,
    pub shutdown: PincPeerShutdown,
    se_correlator: p2p::SeQueryCorrelator,
}

impl<S: Store + 'static> PincPeer<S> {
    /// Binds the endpoint and starts every peer task on `config.edge`, so
    /// the caller may be on any thread or executor.
    pub async fn start(
        store: Arc<S>,
        database: Arc<db::DB<S>>,
        event_bus: Arc<dyn events::Bus>,
        config: PincPeerConfig,
    ) -> p2p::Result<Self> {
        let edge = config.edge.clone();
        edge.spawn(Self::assemble(store, database, event_bus, config))
            .await
            .map_err(|error| {
                p2p::Error::Transport(format!(
                    "pinc peer did not start on the network runtime: {error}"
                ))
            })?
    }

    async fn assemble(
        store: Arc<S>,
        database: Arc<db::DB<S>>,
        event_bus: Arc<dyn events::Bus>,
        config: PincPeerConfig,
    ) -> p2p::Result<Self> {
        let PincPeerConfig {
            endpoint,
            edge,
            document_acp,
            strict_replicated_doc_access,
            sync,
            access_mode,
            rebroadcast_on_merge,
            max_merge_depth,
            retry_schedule,
            load_persisted_collections,
            replicator_push_options,
        } = config;

        let (transport, events, replicators, endpoint_task) =
            defra_pinc::spawn_endpoint(endpoint, edge.clone()).await?;
        let local_peer_id = transport.local_peer_id().to_string();
        p2p::model_trace!(Crash {
            node: &local_peer_id,
        });

        let blockstore = Arc::new(
            PincBlockstore::new(Arc::clone(&store), true)
                .with_max_txn_retries(database.options().max_txn_retries())
                .placed_on(database.pool()),
        );
        let serve_acp = Arc::new(LateBoundServeAcp::new());
        let (mut coordinator, sync_events) =
            match p2p::sync::SyncCoordinator::with_head_provider_and_serve_gate(
                transport.clone(),
                Arc::clone(&blockstore),
                p2p::network::Network::on(edge.clone()),
                sync,
                access_mode,
                replicators,
                p2p::sync::pooled_collection_store(
                    Arc::new(
                        p2p::sync::P2PCollectionStore::new(Arc::clone(&store))
                            .with_max_txn_retries(database.options().max_txn_retries()),
                    ),
                    database.pool(),
                ),
                Arc::new(db::merge::create_head_provider(Arc::clone(&database))),
                Arc::new(replication_filter::QueryReplicationFilterMatcher::new()),
                DbBlockClassifier::new_arc(Arc::clone(&database)),
                Arc::clone(&serve_acp),
            )
            .await
            {
                Ok(coordinator) => coordinator,
                Err(error) => {
                    stop_endpoint(&transport, endpoint_task).await;
                    return Err(error);
                }
            };

        let failure_rx = db::merge::attach_failure_channel(&mut coordinator, 1024);
        let coordinator = Arc::new(coordinator);
        // Its durable registrations change on the database's worker pool.
        coordinator
            .install_pending_dag_store(p2p::sync::pooled_pending_store(
                Arc::new(
                    p2p::sync::PendingDagStore::new(Arc::clone(&store))
                        .with_max_txn_retries(database.options().max_txn_retries()),
                ),
                database.pool(),
            ))
            .await;

        // The requester's DID has to come from the pinc handshake: the KMS
        // release policy checks the transport-authenticated identity, and an
        // anonymous one is denied every policy-bound document.
        #[cfg(feature = "kms")]
        let kms_transport = match p2p::kms::PubsubKeyTransport::new(
            transport.clone(),
            Arc::new(defra_pinc::PincPeerIdentityResolver::new(transport.clone())),
            edge.clone(),
        )
        .await
        {
            Ok(kms_transport) => kms_transport,
            Err(error) => {
                coordinator.shutdown().await;
                stop_endpoint(&transport, endpoint_task).await;
                return Err(p2p::Error::Transport(format!(
                    "failed to create KMS transport: {error}"
                )));
            }
        };
        #[cfg(feature = "kms")]
        coordinator.install_kms_transport(Arc::clone(&kms_transport));

        // The topics every node joins at start, as Go's nodes do, so peers see each other
        // JOINED on them. Raw: pinc has no pubsub doc-sync service, so what
        // arrives is never decoded as a head hint.
        for topic in [p2p::DOC_SYNC_TOPIC, p2p::SYNC_BRANCHABLE_TOPIC] {
            if let Err(error) = transport.subscribe_raw(topic.to_owned()).await {
                tracing::warn!(topic, %error, "failed to subscribe to default topic");
            }
        }

        let replication = db::merge::create_replication_stack_with_max_merge_depth(
            Arc::clone(&database),
            Arc::clone(&blockstore),
            Arc::clone(&coordinator),
            max_merge_depth,
        );
        let doc_pusher_acp = Arc::new(
            DbTransportDocPusher::new(
                Arc::clone(&database),
                transport.clone(),
                coordinator.head_hint_car_authority(),
            )
            .with_retry_schedule(retry_schedule.clone()),
        );

        // Inbound events queue in the endpoint's channel until the handler
        // below drains them, so binding ACP here precedes every merge and serve.
        serve_acp.set(ServeAcp {
            resolver: Arc::new(defra_pinc::PincPeerIdentityResolver::new(transport.clone())),
            gate: DbBlockReadGate::new_arc(Arc::clone(&document_acp)),
        });
        coordinator.set_document_acp(Arc::clone(&document_acp));
        replication
            .merge_handler
            .set_strict_replicated_doc_access(strict_replicated_doc_access);
        replication
            .merge_handler
            .set_document_acp(Arc::clone(&document_acp));
        doc_pusher_acp.set_document_acp(Arc::clone(&document_acp));
        replication.broadcast_mutator.set_document_acp(document_acp);
        // Its storage calls run on the database's worker pool.
        let doc_pusher: Arc<dyn TransportDocPusher> =
            crate::pooled(doc_pusher_acp, database.pool());

        if load_persisted_collections {
            match db::merge::load_persisted_collections(&coordinator).await {
                Ok(0) => {}
                Ok(count) => tracing::info!(count, "loaded persisted P2P collection subscriptions"),
                Err(error) => {
                    tracing::warn!(%error, "failed to load persisted P2P collections")
                }
            }
        }

        let se_correlator = p2p::SeQueryCorrelator::new();
        let manage_correlator = p2p::ManageCorrelator::new();
        let manage_query_correlator = p2p::ManageQueryCorrelator::new();
        let manage_hooks = crate::manage::hooks::new_manage_hooks_cell();

        let replication_coordinator = Arc::clone(&coordinator);
        // Each merge runs on the database's worker pool, on its document's
        // worker.
        let merge_handler = replication.merge_handler_on_pool();
        let replication_bus = Arc::clone(&event_bus);
        let replication_peer = local_peer_id.clone();
        tracing::info!("Starting replication loop for P2P sync (pinc)");
        let replication_task = edge.spawn_service(async move {
            p2p::sync::ReplicationLoop::run(
                replication_coordinator,
                sync_events,
                merge_handler,
                p2p::sync::ReplicationConfig {
                    rebroadcast_on_merge,
                    ..p2p::sync::ReplicationConfig::default()
                },
                move |result| {
                    crate::publish_replication_result(
                        replication_bus.as_ref(),
                        &replication_peer,
                        result,
                    )
                },
            )
            .await;
        });

        // Owned by the coordinator, so shutdown drains them before the store
        // they hold is released (#1309). Started only once the replication loop
        // consumes sync events.
        let resync_coordinator = Arc::clone(&coordinator);
        coordinator.spawn_background_task("pending_dag_resync", async move {
            resync_coordinator
                .run_pending_dag_resync(std::time::Duration::from_secs(60))
                .await;
        });
        // A replication filter also judges pushes before their blocks are
        // fetched; the merge keeps its own check.
        if let Some(gate) = db::receive_gate::DbReceiveGate::new(Arc::clone(&database)) {
            coordinator.set_receive_gate(Arc::new(gate));
        }
        let retry_clock_coordinator = Arc::clone(&coordinator);
        coordinator.spawn_background_task("pending_dag_retry_clock", async move {
            retry_clock_coordinator
                .run_pending_dag_retry_clock(std::time::Duration::from_secs(2))
                .await
                .expect("retry interval is nonzero");
        });

        // The fallback for a deferred verdict no arrival can release: one
        // named nothing, one past the index's capacity, and everything the
        // index held before this process started.
        // It runs on the database's worker pool and ends with the
        // coordinator's shutdown.
        drop(db::merge::governance::start_governance_sweep(
            Arc::clone(&replication.merge_handler_inner),
            db::merge::governance::SWEEP_INTERVAL,
            coordinator.shutdown_handle(),
        ));

        let event_handler_task = spawn_event_handler(
            events,
            Arc::clone(&coordinator),
            Arc::clone(&store),
            database.peerstore(),
            Arc::clone(&event_bus),
            transport.clone(),
            manage_hooks.clone(),
            se_correlator.clone(),
            &edge,
        );

        let failure_recorder_task = crate::spawn_failure_recorder(
            database
                .peerstore()
                .with_retry_schedule(retry_schedule.clone()),
            failure_rx,
        );
        let se_repusher =
            db::merge::pooled_se_repusher(replication.broadcast_mutator.clone(), database.pool());
        replication
            .merge_handler_inner
            .set_se_repusher(se_repusher.clone());
        let retry_loop_task = crate::spawn_retry_loop(
            database.peerstore().with_retry_schedule(retry_schedule),
            transport.clone(),
            Arc::clone(&doc_pusher),
            Some(se_repusher),
            &edge,
        );

        let version_syncer = DbTransportVersionSyncer::new_arc(
            blockstore,
            Arc::clone(&replication.merge_handler_inner),
            Arc::clone(&database),
            transport.clone(),
        );
        let restored_doc_ids =
            crate::restore_pinc_p2p_state(Arc::clone(&store), &transport, &coordinator).await;
        p2p::model_trace!(RestoreReplicators);
        let mut adapter = PincP2PAdapter::with_full_context(
            transport.clone(),
            Arc::clone(&coordinator),
            Arc::clone(&doc_pusher),
            event_bus,
            Some(version_syncer),
            db::node_access_checker(database),
        );
        if let Some(options) = replicator_push_options {
            adapter = adapter.with_replicator_push_options_state(options);
        }
        adapter.set_initial_tracked_documents(restored_doc_ids);

        let manage = PincManageChannel {
            hooks: manage_hooks,
            requester: Arc::new(crate::manage::client::ManageClient::new(
                transport.clone(),
                manage_correlator.clone(),
                manage_query_correlator.clone(),
            )),
            correlator: manage_correlator,
            query_correlator: manage_query_correlator,
        };

        tracing::info!(peer_id = %local_peer_id, "pinc peer started");
        Ok(Self {
            shutdown: PincPeerShutdown::new(
                transport.clone(),
                coordinator.shutdown_handle(),
                endpoint_task,
                retry_loop_task,
                vec![event_handler_task, replication_task],
                failure_recorder_task,
                edge.clone(),
            ),
            transport,
            coordinator,
            replication,
            doc_pusher,
            ops: Arc::new(adapter),
            manage,
            #[cfg(feature = "kms")]
            kms_transport,
            local_peer_id,
            se_correlator,
        })
    }

    /// What binds the KMS built after the peer: merges decrypt with it, and
    /// a key's arrival wakes the merges parked waiting for it.
    #[cfg(feature = "kms")]
    pub fn kms_binder(&self) -> impl FnOnce(Arc<dyn kms::KmsService>) + Send + 'static {
        let coordinator = Arc::downgrade(&self.coordinator);
        let merge_handler = Arc::clone(&self.replication.merge_handler_inner);
        move |kms| {
            kms.set_key_arrival_listener(crate::key_arrival_promoter(coordinator));
            merge_handler.set_kms(kms);
        }
    }

    /// The searchable-encryption query transport, fanning encrypted queries
    /// to this peer's replicators under `key`.
    pub fn se_query_transport(
        &self,
        key: db::merge::SeKeyHandle,
    ) -> Arc<dyn query::SeQueryTransport> {
        Arc::new(db::merge::DbMergeSeQueryTransport::new(
            self.transport.clone(),
            self.se_correlator.clone(),
            Arc::clone(self.coordinator.replicators()),
            key,
            self.coordinator.network(),
        ))
    }
}

async fn stop_endpoint(transport: &PincTransport, endpoint_task: edge::Task<()>) {
    if let Err(error) = transport.shutdown().await {
        tracing::debug!(%error, "failed to signal pinc endpoint during setup rollback");
    }
    let _ = endpoint_task.await;
}

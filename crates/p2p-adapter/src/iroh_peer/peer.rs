//! Assembling the peer.

use std::sync::Arc;

use p2p::bitswap::{LateBoundServeAcp, ServeAcp};
use p2p::iroh::IrohTransport;
use p2p::P2PTransport;
use storage::corekv::Store;
use storage::stores::Peerstore;

use super::config::IrohPeerConfig;
use super::events::spawn_event_handler;
use super::shutdown::IrohPeerShutdown;
use crate::manage::hooks::ManageHooksCell;
use crate::{
    DbBlockClassifier, DbBlockReadGate, DbTransportDocPusher, DbTransportVersionSyncer,
    IrohP2PAdapter, P2POperations, TransportDocPusher,
};

pub type IrohBlockstore<S> = blockstore::DefraBlockstore<S>;
pub type IrohCoordinator<S> = p2p::sync::IrohSyncCoordinator<IrohBlockstore<S>>;
pub type IrohReplicationStack<S> = db::merge::ReplicationStack<S, IrohBlockstore<S>, IrohTransport>;

/// The management channel: inbound requests are served once `hooks` is
/// populated, and `requester` relays outbound ones.
pub struct ManageChannel {
    pub hooks: ManageHooksCell,
    pub correlator: p2p::ManageCorrelator,
    pub query_correlator: p2p::ManageQueryCorrelator,
    pub requester: Arc<dyn defra_http::ManageRequester>,
}

/// A running iroh peer. Stop it with [`IrohPeerShutdown::shutdown`].
pub struct IrohPeer<S: Store + 'static> {
    pub transport: IrohTransport,
    pub coordinator: Arc<IrohCoordinator<S>>,
    pub replication: IrohReplicationStack<S>,
    pub doc_pusher: Arc<dyn TransportDocPusher>,
    pub ops: Arc<dyn P2POperations>,
    pub manage: ManageChannel,
    #[cfg(feature = "kms")]
    pub kms_transport: Arc<p2p::kms::PubsubKeyTransport<IrohTransport>>,
    pub local_peer_id: String,
    pub shutdown: IrohPeerShutdown,
    #[cfg(not(target_arch = "wasm32"))]
    se_correlator: p2p::SeQueryCorrelator,
}

impl<S: Store + 'static> IrohPeer<S> {
    // Nothing here is Send on wasm32, and nothing needs to be: the browser has
    // one thread.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    pub async fn start(
        store: Arc<S>,
        database: Arc<db::DB<S>>,
        event_bus: Arc<dyn events::Bus>,
        config: IrohPeerConfig,
    ) -> p2p::Result<Self> {
        let IrohPeerConfig {
            endpoint,
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

        let secret_key = endpoint.secret_key.clone();
        let (command_tx, events, replicators, endpoint_task) =
            p2p::iroh::spawn_endpoint(endpoint).await?;
        let transport = IrohTransport::new(command_tx, secret_key);
        let local_peer_id = transport.local_peer_id().to_string();

        let blockstore = Arc::new(IrohBlockstore::new(Arc::clone(&store), true));
        let serve_acp = Arc::new(LateBoundServeAcp::new());
        let (mut coordinator, sync_events) =
            match p2p::sync::SyncCoordinator::with_head_provider_and_serve_gate(
                transport.clone(),
                Arc::clone(&blockstore),
                sync,
                access_mode,
                replicators,
                Arc::new(p2p::sync::P2PCollectionStore::new(Arc::clone(&store))),
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
        coordinator
            .install_pending_dag_store(Arc::new(p2p::sync::PendingDagStore::new(Arc::clone(
                &store,
            ))))
            .await;

        // The requester's DID has to come from the iroh handshake: the KMS
        // release policy checks the transport-authenticated identity, and an
        // anonymous one is denied every policy-bound document.
        #[cfg(feature = "kms")]
        let kms_transport = match p2p::kms::PubsubKeyTransport::new(
            transport.clone(),
            Arc::new(p2p::IrohPeerIdentityResolver::new(transport.clone())),
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
            resolver: Arc::new(p2p::IrohPeerIdentityResolver::new(transport.clone())),
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
        let doc_pusher: Arc<dyn TransportDocPusher> = doc_pusher_acp;

        if load_persisted_collections {
            match db::merge::load_persisted_collections(&coordinator).await {
                Ok(0) => {}
                Ok(count) => tracing::info!(count, "loaded persisted P2P collection subscriptions"),
                Err(error) => {
                    tracing::warn!(%error, "failed to load persisted P2P collections")
                }
            }
        }

        // pubsub_rpc doc-sync and branchable-sync services, so this peer
        // interoperates with Go DefraDB peers over gossip. They live beside the
        // libp2p transport, so an iroh-only build has none to start.
        #[cfg(feature = "libp2p")]
        if let Err(error) = coordinator.start_pubsub_services().await {
            tracing::warn!(%error, "failed to start pubsub_rpc services");
        }

        #[cfg(not(target_arch = "wasm32"))]
        let se_correlator = p2p::SeQueryCorrelator::new();
        let manage_correlator = p2p::ManageCorrelator::new();
        let manage_query_correlator = p2p::ManageQueryCorrelator::new();
        let manage_hooks = crate::manage::hooks::new_manage_hooks_cell();

        let replication_coordinator = Arc::clone(&coordinator);
        let merge_handler = Arc::clone(&replication.merge_handler);
        let replication_bus = Arc::clone(&event_bus);
        let replication_peer = local_peer_id.clone();
        tracing::info!("Starting replication loop for P2P sync (iroh)");
        let replication_task = n0_future::task::spawn(async move {
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
        let sweep_handler = Arc::clone(&replication.merge_handler_inner);
        let sweep_shutdown = coordinator.shutdown_handle();
        coordinator.spawn_background_task("governance_sweep", async move {
            db::merge::governance::run_governance_sweep(
                sweep_handler,
                db::merge::governance::SWEEP_INTERVAL,
                sweep_shutdown,
            )
            .await;
        });

        let event_handler_task = spawn_event_handler(
            events,
            Arc::clone(&coordinator),
            Arc::clone(&store),
            Arc::clone(&event_bus),
            transport.clone(),
            manage_hooks.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            se_correlator.clone(),
        );

        let failure_recorder_task = crate::spawn_failure_recorder(
            Peerstore::new(Arc::clone(&store)).with_retry_schedule(retry_schedule.clone()),
            failure_rx,
        );
        let se_repusher: Arc<dyn db::merge::SeArtifactRepusher> =
            replication.broadcast_mutator.clone();
        #[cfg(not(target_arch = "wasm32"))]
        replication
            .merge_handler_inner
            .set_se_repusher(se_repusher.clone());
        let retry_loop_task = crate::spawn_retry_loop(
            Peerstore::new(Arc::clone(&store)).with_retry_schedule(retry_schedule),
            transport.clone(),
            Arc::clone(&doc_pusher),
            Some(se_repusher),
        );

        let version_syncer = DbTransportVersionSyncer::new_arc(
            blockstore,
            Arc::clone(&replication.merge_handler_inner),
            Arc::clone(&database),
            transport.clone(),
        );
        let restored_doc_ids =
            crate::restore_iroh_p2p_state(Arc::clone(&store), &transport, &coordinator).await;
        let mut adapter = IrohP2PAdapter::with_full_context(
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

        let manage = ManageChannel {
            hooks: manage_hooks,
            requester: Arc::new(crate::manage::client::ManageClient::new(
                transport.clone(),
                manage_correlator.clone(),
                manage_query_correlator.clone(),
            )),
            correlator: manage_correlator,
            query_correlator: manage_query_correlator,
        };

        tracing::info!(peer_id = %local_peer_id, "iroh peer started");
        Ok(Self {
            shutdown: IrohPeerShutdown::new(
                transport.clone(),
                coordinator.shutdown_handle(),
                endpoint_task,
                retry_loop_task,
                vec![event_handler_task, replication_task, failure_recorder_task],
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
            #[cfg(not(target_arch = "wasm32"))]
            se_correlator,
        })
    }

    /// Bind the KMS built after the peer.
    #[cfg(feature = "kms")]
    pub fn wire_kms(&self, kms: Arc<dyn kms::KmsService>) {
        self.replication.merge_handler_inner.set_kms(kms);
    }

    /// The searchable-encryption query transport, fanning encrypted queries
    /// to this peer's replicators under `key`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn se_query_transport(
        &self,
        key: db::merge::SeKeyHandle,
    ) -> Arc<dyn query::SeQueryTransport> {
        Arc::new(db::merge::DbMergeSeQueryTransport::new(
            self.transport.clone(),
            self.se_correlator.clone(),
            Arc::clone(self.coordinator.replicators()),
            key,
        ))
    }
}

async fn stop_endpoint(transport: &IrohTransport, endpoint_task: n0_future::task::JoinHandle<()>) {
    if let Err(error) = transport.shutdown().await {
        tracing::debug!(%error, "failed to signal iroh endpoint during setup rollback");
    }
    let _ = endpoint_task.await;
}

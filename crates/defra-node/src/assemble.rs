//! The one node assembly, which [`crate::NodeBuilder::build`] ends in.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use regolith::sync::Notify;

#[cfg(not(target_arch = "wasm32"))]
use crate::background_tasks::BackgroundTasks;
use crate::config::{NodeConfig, TransportConfig};
use crate::network::NodeEdge;
use crate::{acp_ops, db_impls, node_acp, node_identity, timeout_secs};
use crate::{BlockOps, Error, Node, Result, SchemaOps};

#[cfg(feature = "p2p")]
use crate::access_hooks::AccessHooks;
#[cfg(feature = "otel")]
use crate::TelemetryHandle;
#[cfg(feature = "http")]
use crate::TransactionCleanupConfig;

/// What a build takes besides its [`NodeConfig`]: the settings that are not
/// plain values, and the ones only [`crate::NodeBuilder`] sets.
#[derive(Default)]
pub(crate) struct Extras {
    pub embedding_url: Option<String>,
    pub embedding_model: Option<String>,
    pub embedding_api_key: Option<String>,
    #[cfg(feature = "http")]
    pub transaction_cleanup: Option<TransactionCleanupConfig>,
    #[cfg(feature = "http")]
    pub serves_http: bool,
    #[cfg(feature = "p2p")]
    pub access_hooks: Option<AccessHooks>,
    #[cfg(feature = "otel")]
    pub telemetry: Option<TelemetryHandle>,
}

/// The worker pool a node's data path runs on: `config` (the pool's
/// defaults when unset), with the network runtime's timer running its
/// stuck-poll monitor when the node has one.
fn start_node_pool(
    config: Option<workers::Config>,
    edge: Option<&edge::Edge>,
) -> Result<workers::Pool> {
    storage::start_pool(
        config.unwrap_or_default(),
        edge.map(edge::Edge::pool_monitor),
    )
    .map_err(Error::WorkerPoolStart)
}

/// Access hooks need the iroh transport.
#[cfg(feature = "p2p")]
pub(crate) fn check_access_hooks(config: &NodeConfig, hooks: Option<&AccessHooks>) -> Result<()> {
    if hooks.is_some() && !matches!(config.transport, TransportConfig::Iroh(_)) {
        return Err(Error::AccessHooksNeedIroh);
    }
    Ok(())
}

pub(crate) async fn assemble<O>(
    open_store: O,
    config: NodeConfig,
    workers: Option<workers::Config>,
    extras: Extras,
) -> Result<Node>
where
    O: FnOnce(&workers::Pool) -> Result<Arc<storage::RegolithStore>>,
{
    let NodeConfig {
        peer_keyring,
        persistence,
        transport,
        edge: given_edge,
        signing,
        document_acp: document_acp_config,
        query_limits,
        max_txn_retries,
        chunk_size,
        node_acp_enabled,
        query_timeout,
        introspection_cache,
        retention,
        retention_rule,
        replication_filter,
    } = config;
    let introspection_cache = introspection_cache.unwrap_or(true);
    #[cfg(not(any(feature = "p2p", feature = "libp2p")))]
    let _ = (&peer_keyring, &transport);

    let event_bus: Arc<dyn events::Bus> = Arc::new(events::ChannelBus::default());
    let (raw_identity, node_identity_did) = node_identity::create_node_identity(&signing)?;
    let node_p2p_identity = raw_identity.map(Arc::new);
    let node_query_identity = node_identity_did
        .as_ref()
        .map(|did| identity::Did::new(did.clone()))
        .transpose()
        .map_err(Error::InvalidNodeIdentityDid)?;

    // Everything the config can refuse is checked before anything opens, so a
    // refusal has nothing to close.
    let nac_owner = nac_owner(
        node_acp_enabled,
        &node_identity_did,
        node_p2p_identity.is_some(),
    )?;
    #[cfg(any(feature = "p2p", feature = "libp2p"))]
    match &transport {
        #[cfg(feature = "libp2p")]
        TransportConfig::Libp2p(libp2p) => drop(libp2p.sync.retry_schedule()?),
        #[cfg(feature = "p2p")]
        TransportConfig::Iroh(iroh) => drop(iroh.sync.retry_schedule()?),
        TransportConfig::None => {}
    }

    // The node's network runtime comes next, so a failure to start it
    // leaves no network work behind.
    #[cfg(feature = "http")]
    let serves_http = extras.serves_http;
    #[cfg(not(feature = "http"))]
    let serves_http = false;
    let runs_transport = !matches!(transport, TransportConfig::None);
    let network = NodeEdge::start(
        given_edge,
        node_identity_did.is_some() || serves_http || runs_transport,
    )?;

    // The data path's worker pool comes before the store, which places its
    // compaction threads by the pool's plan; the network runtime's timer,
    // when the node has one, runs the pool's stuck-poll monitor.
    let pool = start_node_pool(workers, network.as_ref().map(NodeEdge::edge))?;
    let store = open_store(&pool)?;

    let mut db_options = db::DbOptions::default().with_pool(pool);
    if let Some(url) = extras.embedding_url {
        db_options = db_options.with_embedding_url(url);
    }
    if let Some(model) = extras.embedding_model {
        db_options = db_options.with_embedding_model(model);
    }
    if let Some(api_key) = extras.embedding_api_key {
        db_options = db_options.with_embedding_api_key(api_key);
    }
    if let Some(retries) = max_txn_retries {
        db_options = db_options.with_max_txn_retries(retries);
    }
    if let Some(size) = chunk_size {
        db_options = db_options.with_chunk_size(size);
    }
    if let Some(rule) = retention_rule {
        db_options = db_options.with_retention_rule(rule.0);
    } else if let Some(rule) = retention {
        db_options = db_options.with_retention_rule(Arc::new(rule));
    }
    if let Some(filter) = replication_filter {
        db_options = db_options.with_replication_filter(filter.0);
    }
    if let Some(identity) = node_p2p_identity.as_ref() {
        db_options = db_options.with_node_identity_arc(identity.clone());
    }
    let embedding_config = db_options.embedding_config();
    #[cfg(not(target_arch = "wasm32"))]
    let transaction_stats = storage::Store::transaction_stats_handle(store.as_ref());

    let mut database = db::DB::open_from_arc_with_options(store.clone(), db_options)
        .await
        .map_err(Error::DatabaseOpen)?;
    database.set_event_bus(event_bus.clone());
    if let Some(network) = network.as_ref() {
        database.set_network(network.edge().clone());
    }
    #[cfg(feature = "p2p")]
    if let Some(hooks) = &extras.access_hooks {
        hooks.install(&database);
    }
    let database = Arc::new(database);

    // Node access control: always present, off until enabled. At startup it is
    // enabled only when asked for, and then the node identity owns the
    // policy, so DB-layer node operations run by the node itself are always
    // allowed (the node-DID bypass in check_node_access).
    let nac_manager = close_on_error(
        &database,
        node_acp::create_nac_manager(store.clone(), nac_owner.as_ref()).await,
    )
    .await?;
    database.set_nac_manager(nac_manager.clone());

    let acp_setup = close_on_error(
        &database,
        node_acp::create_document_acp(
            store.clone(),
            persistence,
            &document_acp_config,
            network.as_ref().map(NodeEdge::edge),
        )
        .await,
    )
    .await?;
    let document_acp = acp_setup.document_acp.clone();
    #[cfg(feature = "vera")]
    let remote_acp = acp_setup.vera_acp.is_some();
    #[cfg(not(feature = "vera"))]
    let remote_acp = false;
    #[cfg(any(feature = "p2p", feature = "libp2p"))]
    let strict_replicated_doc_access = remote_acp;

    // P2P setup (affects mutator choice)
    #[cfg(any(feature = "p2p", feature = "libp2p"))]
    let mut p2p_setup = {
        let setup = match (&transport, network.as_ref()) {
            #[cfg(feature = "libp2p")]
            (TransportConfig::Libp2p(libp2p), Some(network)) => {
                let libp2p = &crate::config::Libp2pConfig {
                    orphan_sweep: persistence == crate::Persistence::Persistent,
                    ..libp2p.clone()
                };
                let schedule = libp2p.sync.retry_schedule()?;
                crate::libp2p_runtime::setup_libp2p(
                    store.clone(),
                    database.clone(),
                    event_bus.clone(),
                    libp2p,
                    libp2p.sync.sync_config(),
                    schedule,
                    peer_keyring.as_ref(),
                    network.edge(),
                )
                .await
                .map(Some)
            }
            #[cfg(feature = "p2p")]
            (TransportConfig::Iroh(iroh), Some(network)) => crate::p2p_runtime::setup_iroh(
                store.clone(),
                database.clone(),
                event_bus.clone(),
                iroh,
                node_p2p_identity.clone(),
                document_acp.clone(),
                strict_replicated_doc_access,
                peer_keyring.as_ref(),
                network.edge(),
            )
            .await
            .map(Some),
            // A transport always has a runtime here: it is network work.
            _ => Ok(None),
        };
        match setup {
            Ok(setup) => setup,
            Err(error) => {
                if let Err(close_error) = database.close().await {
                    tracing::warn!(%close_error, "failed to close database after P2P setup error");
                }
                return Err(error);
            }
        }
    };

    #[cfg(any(feature = "p2p", feature = "libp2p"))]
    if let Some(setup) = p2p_setup.as_mut() {
        #[cfg(feature = "libp2p")]
        if let Some(wire_document_acp) = setup.wire_document_acp.take() {
            wire_document_acp(document_acp.clone(), strict_replicated_doc_access);
        }
        // Populate the manage-channel serve deps now that the controller and
        // NAC manager exist; until this fires the event loop drops inbound
        // manage requests rather than serving them unauthenticated.
        let _ = setup
            .manage_hooks
            .set(defra_p2p_adapter::manage::hooks::ManageHooks {
                ops: setup.manage_controller.clone(),
                nac: nac_manager.clone(),
                correlator: setup.manage_correlator.clone(),
                query_correlator: setup.manage_query_correlator.clone(),
            });
        // A bootstrap peer that cannot be reached does not stop the node,
        // as in Go: the node starts and the peer can be connected later.
        #[cfg(feature = "libp2p")]
        if let TransportConfig::Libp2p(libp2p) = &transport {
            for addr in &libp2p.bootstrap_peers {
                if let Err(error) = setup.system.ops().connect_peer(addr).await {
                    tracing::warn!(%addr, %error, "failed to connect to bootstrap peer");
                }
            }
        }
    }

    // Choose mutator: BroadcastMutator if P2P, AutoCommitMutator otherwise
    #[cfg(any(feature = "p2p", feature = "libp2p"))]
    let mutator: Arc<dyn query::DocMutator> = match p2p_setup.as_ref() {
        Some(setup) => setup.mutator.clone(),
        None => Arc::new(db::AutoCommitMutator::new(database.clone())),
    };
    #[cfg(not(any(feature = "p2p", feature = "libp2p")))]
    let mutator: Arc<dyn query::DocMutator> =
        Arc::new(db::AutoCommitMutator::new(database.clone()));

    let fetcher = db::LensedAutoCommitFetcher::new(database.clone());
    let provider: Arc<dyn query::CollectionProvider> =
        db::DbCollectionProvider::new_arc(database.clone());

    #[cfg(any(feature = "p2p", feature = "libp2p"))]
    let txn_broadcaster: Option<Arc<dyn db::event::emission::TxnBroadcaster>> = p2p_setup
        .as_ref()
        .map(|setup| setup.txn_broadcaster.clone());
    #[cfg(not(any(feature = "p2p", feature = "libp2p")))]
    let txn_broadcaster: Option<Arc<dyn db::event::emission::TxnBroadcaster>> = None;

    let registry = Arc::new(match txn_broadcaster {
        Some(broadcaster) => {
            db::DbTransactionRegistry::with_broadcaster(database.clone(), broadcaster)
        }
        None => db::DbTransactionRegistry::new(database.clone()),
    });

    #[cfg(feature = "http")]
    let txn_cleanup_task = extras.transaction_cleanup.map(|cleanup| {
        tracing::info!(
            max_idle_age_secs = cleanup.max_idle_age.as_secs(),
            sweep_interval_secs = cleanup.sweep_interval.as_secs(),
            "Transaction idle cleanup worker enabled"
        );
        registry.start_stale_transaction_cleanup(cleanup.max_idle_age, cleanup.sweep_interval)
    });

    #[cfg(feature = "managed-p2p")]
    {
        // The KMS comes once document ACP and NAC exist: a blockstore-backed key
        // store, the NAC/DAC release policy and, when the node has P2P, its
        // pubsub transport. Without it encrypted blocks replicate but no peer can
        // fetch the DEK, so encrypted documents never become readable on replicas.
        let kms: Arc<dyn kms::KmsService> = {
            // The KMS serves DEKs for ANY encrypted write by reading and writing
            // the node's durable encstore->blockstore, not a RAM-only map. The DB
            // owns the blockstore Arc (set_kms_blockstore) so the adapter can hold
            // a Weak and avoid the lock-pinning cycle (#976).
            let kms_blockstore = database.set_kms_blockstore(Arc::new(
                blockstore::DefraBlockstore::new(store.clone(), true)
                    .with_max_txn_retries(database.options().max_txn_retries())
                    .placed_on(database.pool()),
            ));
            let enc_block_store: Arc<dyn kms::EncBlockStore> =
                Arc::new(db::DbEncBlockStore::new(database.clone(), kms_blockstore));
            let key_store: Arc<dyn kms::KeyStore> =
                Arc::new(kms::BlockstoreKeyStore::new(enc_block_store));
            let doc_lookup: Arc<dyn kms::DocCollectionLookup> =
                Arc::new(db::DbDocCollectionLookup::new(database.clone()));
            let policy = Arc::new(kms::NacDacPolicy::new(document_acp.clone(), doc_lookup));
            policy.set_node_acp(Arc::new(db::DbNodeAcpRead::new(nac_manager.clone())));

            // Node identity used for cross-peer DEK requests. Anonymous nodes use
            // a stable placeholder DID.
            let kms_node_did = database.node_did().unwrap_or_else(|| {
                identity::Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")
                    .expect("static anonymous DID parses")
            });

            #[cfg(any(feature = "p2p", feature = "libp2p"))]
            let transports: Vec<Arc<dyn kms::KeyTransport>> = match p2p_setup.as_ref() {
                Some(setup) => vec![setup.kms_transport.clone()],
                None => vec![],
            };
            #[cfg(not(any(feature = "p2p", feature = "libp2p")))]
            let transports: Vec<Arc<dyn kms::KeyTransport>> = vec![];

            Arc::new(kms::DefraKms::with_inputs(
                key_store,
                transports,
                policy as Arc<dyn kms::AccessPolicy>,
                Arc::new(db::DbBlockDocIDResolver::new(database.clone())),
                kms_node_did,
                network
                    .as_ref()
                    .map(|network| kms::KmsInputs::on(network.edge()))
                    .unwrap_or_default(),
            ))
        };

        // Bind this node's transport peer id into the KMS so served ECIES
        // replies carry the correct AAD peer id, and wire the KMS into the
        // transport's serve handler and merge handler.
        #[cfg(any(feature = "p2p", feature = "libp2p"))]
        if let Some(setup) = p2p_setup.as_mut() {
            kms.set_local_peer_id(setup.local_peer_id.clone());

            // A Weak ref breaks the transport<->kms Arc cycle (transport holds
            // handler -> kms -> transports -> transport).
            struct KmsServeHandler {
                kms: std::sync::Weak<dyn kms::KmsService>,
            }
            #[async_trait::async_trait]
            impl kms::IncomingHandler for KmsServeHandler {
                async fn handle(
                    &self,
                    from: kms::PeerIdentity,
                    req: kms::FetchEncryptionKeyRequest,
                ) -> kms::Result<kms::FetchEncryptionKeyReply> {
                    match self.kms.upgrade() {
                        Some(kms) => kms.serve_request(from, req).await,
                        None => Err(kms::Error::Internal("kms dropped".into())),
                    }
                }
            }
            setup
                .kms_transport
                .install_handler(Arc::new(KmsServeHandler {
                    kms: Arc::downgrade(&kms),
                }));

            if let Some(wire_kms) = setup.wire_kms.take() {
                wire_kms(kms.clone());
            }
        }

        // Wire the KMS into the write path (DB-held, read by doc_mutator).
        database.set_kms(kms);
    }

    // Assemble query runner
    let pool = database.pool().cloned();
    let query_runner =
        query::QueryRunner::with_arc_registry_and_provider(fetcher, provider, registry.clone())
            .with_mutator(mutator)
            .with_collection_truncator(db::DbCollectionTruncator::new_arc(database.clone()))
            .with_acp(document_acp.clone())
            .with_lens_store(database.lens_store().clone())
            .with_query_limits(query_limits)
            .with_introspection_cache(introspection_cache)
            .with_deadline_pool(pool.clone());
    let query_runner = if let Some(timeout) = query_timeout {
        query_runner.with_query_timeout(timeout_secs(timeout))
    } else {
        query_runner
    };
    // Wire the SE remote query transport so this node can act as an SE query
    // OWNER, fanning encrypted_<Collection> queries to replicators (#976).
    #[cfg(any(feature = "p2p", feature = "libp2p"))]
    let query_runner = match p2p_setup
        .as_ref()
        .and_then(|setup| setup.se_transport.clone())
    {
        Some(se_transport) => query_runner.with_se_transport(se_transport),
        None => query_runner,
    };
    #[cfg(feature = "p2p")]
    let query_runner = match &extras.access_hooks {
        Some(hooks) => hooks.apply(query_runner),
        None => query_runner,
    };

    // GraphQL requests run on the database's worker pool, routed there once
    // at the boundary.
    let runner = query::pooled::PooledExecutor::wrap(Arc::new(query_runner), pool.as_ref());

    let local_zanzibar_store = acp_setup.local_zanzibar_store.clone();
    #[cfg(feature = "vera")]
    let vera_acp = acp_setup.vera_acp.clone();
    #[cfg(all(feature = "vera", not(target_arch = "wasm32")))]
    let redrive = acp_setup.redrive;
    #[cfg(feature = "vera")]
    let policy_lookup =
        acp_ops::PolicyLookup::new(acp_setup.local_zanzibar_store, acp_setup.vera_acp);
    #[cfg(not(feature = "vera"))]
    let policy_lookup = acp_ops::PolicyLookup::new(acp_setup.local_zanzibar_store);
    database.set_policy_lookup(Arc::new(policy_lookup.clone()));
    let schema_ops: Arc<dyn SchemaOps> = Arc::new(db_impls::DbSchemaOps::new(
        database.clone(),
        query_limits,
        document_acp.clone(),
        policy_lookup.clone(),
    ));
    let acp_ops: Arc<dyn acp_ops::AcpOps> = Arc::new(acp_ops::DbAcpOps::new(
        database.clone(),
        document_acp.clone(),
        policy_lookup,
        event_bus.clone(),
    ));
    // Started last: nothing after this point fails, so a failed build
    // never leaves a task holding the store open. The vector sweep rebuilds
    // HNSW graphs and merges DiskANN pending inserts; without it a DiskANN
    // index never merges.
    #[cfg(not(target_arch = "wasm32"))]
    let background_tasks = Arc::new(BackgroundTasks::new([
        database.clone().start_vector_rebuild_task(),
        database.clone().start_downsample_task(),
    ]));
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(replay) = database.clone().start_parked_replay() {
        background_tasks.push(replay);
    }
    #[cfg(all(feature = "vera", not(target_arch = "wasm32")))]
    if let Some(start_redrive) = redrive {
        background_tasks.push(start_redrive(database.pool()));
    }
    let block_ops: Arc<dyn BlockOps> = Arc::new(db_impls::DbBlockOps::new(
        database.clone(),
        document_acp.clone(),
        registry.clone(),
        node_query_identity.clone(),
    ));

    #[cfg(any(feature = "p2p", feature = "libp2p"))]
    let p2p = p2p_setup.map(|setup| setup.system);
    #[cfg(all(feature = "managed-p2p", not(any(feature = "p2p", feature = "libp2p"))))]
    let p2p: Option<Arc<crate::ManagedP2PSystem>> = None;

    Ok(Node {
        database,
        txn_registry: registry,
        nac_manager,
        local_zanzibar_store,
        #[cfg(feature = "vera")]
        vera_acp,
        query_limits,
        runner,
        event_bus,
        schema_ops,
        block_ops,
        acp_ops,
        document_acp,
        embedding_config,
        node_identity_did,
        node_query_identity,
        network,
        pool,
        remote_acp,
        #[cfg(not(target_arch = "wasm32"))]
        transaction_stats,
        #[cfg(feature = "http")]
        txn_cleanup_task: crate::single_slot(txn_cleanup_task),
        #[cfg(not(target_arch = "wasm32"))]
        background_tasks,
        #[cfg(feature = "managed-p2p")]
        p2p,
        shutdown_started: AtomicBool::new(false),
        shutdown_finished: AtomicBool::new(false),
        shutdown_notify: Notify::new(),
        #[cfg(feature = "otel")]
        telemetry: crate::single_slot(extras.telemetry),
    })
}

/// The identity that owns node access control when it is enabled at startup.
fn nac_owner(
    enabled: bool,
    node_identity_did: &Option<String>,
    has_local_key: bool,
) -> Result<Option<identity::Did>> {
    if !enabled {
        return Ok(None);
    }
    let owner = node_identity_did
        .as_deref()
        .ok_or(Error::NodeAcpNeedsIdentity)
        .and_then(|did| identity::Did::new(did).map_err(Error::InvalidNodeAcpIdentity))?;
    // The database's node DID is the DID of the local key, so the owner holds
    // one exactly when the node has a local key.
    if !has_local_key {
        return Err(Error::NodeAcpNeedsLocalKey);
    }
    Ok(Some(owner))
}

/// `result`, after closing `database` when it is an error: a build that
/// fails leaves nothing holding the store open.
async fn close_on_error<S, T>(database: &Arc<db::DB<S>>, result: Result<T>) -> Result<T>
where
    S: storage::corekv::Store + 'static,
{
    if result.is_err() {
        if let Err(error) = database.close().await {
            tracing::warn!(%error, "failed to close database after a failed build");
        }
    }
    result
}

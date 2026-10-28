use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use defra_core::RequestContext;

use crate::error::{Error, Result};
use crate::node_acp::{create_document_acp, create_nac_manager};
use crate::node_identity::create_node_identity;
use crate::node_tasks::BackgroundTasks;
#[cfg(feature = "iroh")]
use crate::IrohConfig;
#[cfg(feature = "libp2p")]
use crate::Libp2pConfig;
#[cfg(feature = "vera")]
use crate::{DocumentAcpConfig, VeraConfig};
use crate::{
    EmbeddedNodeConfig, EmbeddedStore, ManagedP2PSystem, Persistence, SigningConfig, SigningKey,
    TransportConfig,
};
#[cfg(feature = "libp2p")]
use kovan_queue::seg_queue::SegQueue;
#[cfg(any(feature = "libp2p", feature = "iroh"))]
use p2p::sync::SyncConfig;
use regolith::sync::Notify;

#[cfg(feature = "libp2p")]
pub(crate) type EmbeddedBlockstore<S> = blockstore::DefraBlockstore<S>;
#[cfg(feature = "libp2p")]
pub(crate) type EmbeddedMergeHandler<S> = db::merge::AcpMergeHandler<S, EmbeddedBlockstore<S>>;
type EmbeddedTxnRegistry<S> = db::DbTransactionRegistry<S>;
#[cfg(feature = "libp2p")]
pub(crate) type WireDocumentAcpCallback = Box<dyn FnOnce(Arc<dyn acp::DocumentACP>, bool) + Send>;
pub(crate) type WireKmsCallback = Box<dyn FnOnce(Arc<dyn kms::KmsService>) + Send>;

/// Resolve the identity acting in `context` into a creator DID for ACP
/// registration. Returns `Ok(None)` when no identity is present (the
/// collection stays public/unregistered, as intended), but fails closed when an
/// identity string is present yet malformed, never silently degrading a
/// permissioned collection to public.
fn resolve_creator_identity(context: &RequestContext) -> Result<Option<identity::Did>> {
    match context.identity() {
        Some(raw) => Ok(Some(
            identity::Did::new(raw).map_err(Error::InvalidActingIdentity)?,
        )),
        None => Ok(None),
    }
}

/// Embedded DefraDB node assembled for native/mobile embedding.
pub struct EmbeddedNode<S: storage::corekv::Store + 'static> {
    pub database: Arc<db::DB<S>>,
    background_tasks: Arc<BackgroundTasks>,
    pub txn_registry: Arc<EmbeddedTxnRegistry<S>>,
    pub query_runner: Arc<dyn query::QueryExecutor>,
    pub nac_manager: Arc<dyn db::NacManagerApi>,
    pub document_acp: Arc<dyn acp::DocumentACP>,
    pub local_zanzibar_store: Option<Arc<dyn acp::ZanzibarStore>>,
    pub event_bus: Arc<dyn events::Bus>,
    pub node_identity_did: Option<String>,
    #[cfg(feature = "vera")]
    pub vera_acp: Option<Arc<vera::VeraDocumentACP>>,
    pub query_limits: query::QueryLimits,
    pub p2p: Option<Arc<ManagedP2PSystem>>,
    /// Idempotency guard for [`EmbeddedNode::shutdown`]. Set to `true`
    /// by the first caller of `shutdown()`.
    shutdown_started: AtomicBool,
    /// Marks that shutdown work has finished.
    shutdown_finished: AtomicBool,
    /// Wakes concurrent shutdown callers once teardown is complete.
    shutdown_notify: Notify,
}

impl<S: storage::corekv::Store + 'static> EmbeddedNode<S> {
    pub fn builder() -> NodeBuilder {
        NodeBuilder::default()
    }

    pub fn p2p(&self) -> Option<&Arc<ManagedP2PSystem>> {
        self.p2p.as_ref()
    }

    pub fn background_tasks(&self) -> Arc<BackgroundTasks> {
        self.background_tasks.clone()
    }

    /// [`Self::execute_as`] in [`RequestContext::ambient`], signing with the
    /// node's identity when one is configured.
    pub async fn execute(&self, query_str: &str) -> query::QueryResponse {
        self.execute_as(&self.signing_context(), query_str).await
    }

    /// Execute a GraphQL query or mutation for the request `context` serves.
    pub async fn execute_as(
        &self,
        context: &RequestContext,
        query_str: &str,
    ) -> query::QueryResponse {
        self.query_runner
            .execute(query::QueryRequest::new(query_str).with_context(context.clone()))
            .await
    }

    /// [`RequestContext::ambient`] signing with the node's identity, and
    /// collecting into its batch session, when one is configured.
    fn signing_context(&self) -> RequestContext {
        let context = RequestContext::ambient();
        match defra_core::signing::resolve_signing_config(None, self.node_identity_did.as_deref()) {
            Some(signing) => {
                let batch_session = Some(signing.public_key_hex.clone());
                context
                    .with_signing(Some(signing))
                    .with_batch_session(batch_session)
            }
            None => context,
        }
    }

    /// The context the node's own operations run in: the node's identity acts
    /// in it when one is configured, and [`RequestContext::ambient`] otherwise.
    fn node_context(&self) -> RequestContext {
        let context = RequestContext::ambient();
        match &self.node_identity_did {
            Some(did) => context.with_identity(Some(did.clone())),
            None => context,
        }
    }

    pub async fn add_collection(&self, sdl: &str) -> Result<()> {
        let context = self.node_context();
        let (database, document_acp) = (Arc::clone(&self.database), self.document_acp.clone());
        let sdl = sdl.to_owned();
        self.on_pool(move || async move {
            let collections = query::parse_sdl(&sdl).map_err(Error::SchemaParse)?;
            schema::definition_validation::validate_new_collections(&collections)
                .map_err(Error::SchemaValidation)?;
            let creator = resolve_creator_identity(&context)?;
            database
                .create_collections_atomic_with_acp_registration_as(
                    &context,
                    collections,
                    document_acp,
                    creator,
                )
                .await
                .map_err(Error::CollectionCreate)?;
            Ok(())
        })
        .await
    }

    /// Add an equality encrypted (searchable-encryption) index on `field_name`
    /// of `collection`. Embedded-native equivalent of the CLI
    /// `encrypted_index_add` / HTTP `EncryptedIndexOperations::add_encrypted_index`,
    /// so embedded nodes can write SE artifacts and act as SE query owners (#976).
    pub async fn add_encrypted_index(&self, collection: &str, field_name: &str) -> Result<()> {
        let database = Arc::clone(&self.database);
        let (collection, field_name) = (collection.to_owned(), field_name.to_owned());
        self.on_pool(move || add_encrypted_index(database, collection, field_name))
            .await
    }

    /// Runs the data-path call `make` builds on the database's worker pool (in
    /// place on a worker, or without a pool). The request context the call
    /// acts in travels inside it.
    async fn on_pool<F, Fut, T>(&self, make: F) -> Result<T>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        workers::run_on(self.database.pool(), workers::Target::LeastLoaded, make)
            .await
            .map_err(Error::PoolRun)?
    }

    /// Shut the node down cleanly.
    ///
    /// Order:
    /// 1. **P2P transport** — closes the iroh endpoint or libp2p host
    ///    gracefully and aborts the replication/merge background tasks.
    ///    Addresses the `Endpoint dropped without calling Endpoint::close`
    ///    iroh warning downstream embedders have been hitting.
    /// 2. **Background tasks** — aborts and awaits node-owned tasks.
    /// 3. **Database** — close-guards future transactions (new `begin_txn`
    ///    calls will return `Error::DatabaseClosed`), lets the worker pool
    ///    finish what it admitted and stop, and closes the underlying store.
    ///
    /// This method is **idempotent** — the first caller performs the
    /// work, concurrent callers wait for that teardown to finish and
    /// then return. Safe to call from multiple tasks concurrently.
    ///
    /// Embedded callers should `await` this before dropping the
    /// [`EmbeddedNode`] to ensure clean teardown:
    ///
    /// ```ignore
    /// node.shutdown().await;
    /// drop(node);
    /// ```
    pub async fn shutdown(&self) {
        // Idempotency guard. The first task performs shutdown. Any
        // concurrent caller waits for shutdown to finish before
        // returning so `shutdown().await` consistently means teardown
        // has completed.
        if self.shutdown_started.swap(true, Ordering::SeqCst) {
            self.wait_for_shutdown().await;
            return;
        }

        tracing::info!("embedded node shutdown: begin");

        // 1. P2P first — stops inbound/outbound traffic and awaits the
        //    transport's graceful close path (iroh Endpoint::close /
        //    libp2p Host::shutdown).
        if let Some(p2p) = &self.p2p {
            tracing::info!("embedded node shutdown: stopping p2p");
            p2p.shutdown().await;
        }

        // 2. Background tasks — await cancellation so they release store handles.
        self.background_tasks.shutdown().await;

        // 3. Database — sets the is_closed flag and closes the store.
        //    Log but don't propagate errors: shutdown should complete
        //    even if the store close returns an I/O error, so embedders
        //    can't be blocked from exiting by a closing store.
        tracing::info!("embedded node shutdown: closing database");
        if let Err(error) = self.database.close().await {
            tracing::warn!(error = %error, "EmbeddedNode::shutdown: database close returned an error");
        }

        self.shutdown_finished.store(true, Ordering::SeqCst);
        self.shutdown_notify.notify_waiters();
        tracing::info!("embedded node shutdown: complete");
    }

    /// Returns true if [`EmbeddedNode::shutdown`] has been initiated.
    ///
    /// Does not guarantee that shutdown has finished — just that it has
    /// started. Useful for embedders that want to short-circuit pending
    /// operations when teardown is in progress.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown_started.load(Ordering::SeqCst)
    }

    async fn wait_for_shutdown(&self) {
        loop {
            let notified = self.shutdown_notify.notified();
            if self.shutdown_finished.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

/// Builder for memory/regolith embedded nodes.
/// Adds the encrypted index [`EmbeddedNode::add_encrypted_index`] names, on
/// the worker it runs on.
async fn add_encrypted_index<S: storage::corekv::Store + 'static>(
    database: Arc<db::DB<S>>,
    collection: String,
    field_name: String,
) -> Result<()> {
    use storage::corekv::Key;

    let (collection, field_name) = (collection.as_str(), field_name.as_str());
    let col = database
        .get_collection(collection)
        .map_err(Error::CollectionLookup)?
        .ok_or_else(|| Error::CollectionNotFound(collection.to_owned()))?;
    let schema = col.schema();

    if !schema.fields.iter().any(|f| f.name == field_name) {
        return Err(Error::EncryptedIndexFieldMissing(field_name.to_owned()));
    }
    if schema
        .encrypted_indexes
        .iter()
        .any(|idx| idx.field_name == field_name)
    {
        return Err(Error::EncryptedIndexExists(field_name.to_owned()));
    }

    let txn = database.new_txn(false).await.map_err(Error::TxnCreate)?;
    {
        let mut updated_schema = schema.clone();
        updated_schema
            .encrypted_indexes
            .push(schema::EncryptedIndexDescription::new(field_name));

        let collection_key =
            storage::keys::systemstore::CollectionKey::new(&updated_schema.version_id);
        let schema_data = serde_json::to_vec(&updated_schema).map_err(Error::SchemaSerialize)?;
        let systemstore = txn.systemstore().map_err(Error::SystemstoreOpen)?;
        systemstore
            .set(&collection_key.bytes(), &schema_data)
            .await
            .map_err(Error::SchemaSave)?;
        let name_key = storage::keys::systemstore::CollectionNameKey::new(collection);
        systemstore
            .set(&name_key.bytes(), updated_schema.version_id.as_bytes())
            .await
            .map_err(Error::NameMappingSave)?;
    }
    txn.commit().await.map_err(Error::Commit)?;
    database.reload_cache().await.map_err(Error::CacheReload)?;
    Ok(())
}

#[derive(Default)]
pub struct NodeBuilder {
    data_path: Option<PathBuf>,
    config: EmbeddedNodeConfig,
    workers: Option<workers::Config>,
    at_rest_encryption_key: Option<[u8; 32]>,
    previous_at_rest_encryption_keys: Vec<[u8; 32]>,
    #[cfg(feature = "iroh")]
    access_hooks: Option<crate::AccessHooks>,
}

impl NodeBuilder {
    pub fn data_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.data_path = Some(path.into());
        self
    }

    pub fn with_transport(mut self, transport: TransportConfig) -> Self {
        self.config.transport = transport;
        self
    }

    #[cfg(feature = "libp2p")]
    pub fn with_libp2p(mut self, listen_addr: impl Into<String>) -> Self {
        self.config.transport = TransportConfig::Libp2p(Libp2pConfig {
            listen_addr: listen_addr.into(),
        });
        self
    }

    #[cfg(feature = "iroh")]
    pub fn with_iroh(mut self, config: IrohConfig) -> Self {
        self.config.transport = TransportConfig::Iroh(config);
        self
    }

    /// The worker pool the node's data path runs on. Unset, the pool's own
    /// defaults (`workers::Config::default()`).
    pub fn with_workers(mut self, config: workers::Config) -> Self {
        self.workers = Some(config);
        self
    }

    /// The network runtime the node's networking runs on: its transport,
    /// its database's network work and the pool's stuck-poll monitor. Unset,
    /// a node with a transport runs them on the process's own
    /// ([`edge::on_demand`]), started on first use.
    pub fn with_edge(mut self, edge: edge::Edge) -> Self {
        self.config.edge = Some(edge);
        self
    }

    pub fn enable_signing(mut self) -> Self {
        self.config.signing = SigningConfig::Enabled { key: None };
        self
    }

    /// Keep the transport identity in an application-owned keyring.
    ///
    /// The backend must provide durable storage and its own namespace per node.
    /// Startup rejects existing plaintext peer keys; this does not erase or
    /// migrate historical key material from a database or its backups.
    pub fn with_peer_keyring(mut self, keyring: Arc<dyn keyring::Keyring>) -> Self {
        self.config.peer_keyring = Some(crate::PeerKeyring::new(keyring));
        self
    }

    pub fn with_signing_key(mut self, key: SigningKey) -> Self {
        self.config.signing = SigningConfig::Enabled { key: Some(key) };
        self
    }

    pub fn with_signing_identity_did(mut self, did: impl Into<String>) -> Self {
        self.config.signing = SigningConfig::RegisteredIdentity { did: did.into() };
        self
    }

    #[cfg(feature = "vera")]
    pub fn with_vera(mut self, config: VeraConfig) -> Self {
        self.config.document_acp = DocumentAcpConfig::Vera(config);
        self
    }

    /// Configure Vera ACP when LCD and gRPC use distinct endpoints.
    #[cfg(feature = "vera")]
    pub fn with_vera_lcd(mut self, config: VeraConfig, lcd_address: impl Into<String>) -> Self {
        self.config.document_acp = DocumentAcpConfig::VeraWithLcd {
            config,
            lcd_address: lcd_address.into(),
        };
        self
    }

    pub fn with_query_limits(mut self, limits: query::QueryLimits) -> Self {
        self.config.query_limits = limits;
        self
    }

    /// Install the app's own access control. Only an iroh node accepts it:
    /// `build` fails for any other transport.
    #[cfg(feature = "iroh")]
    pub fn with_access_hooks(mut self, hooks: crate::AccessHooks) -> Self {
        self.access_hooks = Some(hooks);
        self
    }

    /// Enable engine file encryption at rest for the storage backend,
    /// keyed by the given 32-byte AES-256 key. Opt-in; off by default.
    pub fn with_at_rest_encryption_key(mut self, key: [u8; 32]) -> Self {
        self.at_rest_encryption_key = Some(key);
        self
    }

    /// Keys the store's files may still be sealed under after a rotation to
    /// the key given to [`NodeBuilder::with_at_rest_encryption_key`]. They
    /// only open what they sealed; new files use the current key.
    pub fn with_previous_at_rest_encryption_keys<I>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = [u8; 32]>,
    {
        self.previous_at_rest_encryption_keys = keys.into_iter().collect();
        self
    }

    fn with_at_rest_keys(
        &self,
        options: storage::RegolithStoreOptions,
    ) -> Result<storage::RegolithStoreOptions> {
        match self.at_rest_encryption_key {
            Some(key) => options
                .with_encryption_keys(key, self.previous_at_rest_encryption_keys.iter().copied())
                .map_err(Error::AtRestKeys),
            None => Ok(options),
        }
    }

    pub async fn build(mut self) -> Result<EmbeddedNode<EmbeddedStore>> {
        resolve_edge(&mut self.config)?;
        // The data path's worker pool comes before the store, which places
        // its compaction threads by the pool's plan.
        let pool = start_node_pool(self.workers.take(), self.config.edge.as_ref())?;
        let (store, persistence) = if let Some(path) = self.data_path.take() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|source| Error::DataDirCreate {
                    path: path.clone(),
                    source,
                })?;
            }

            #[cfg(feature = "iroh")]
            if let TransportConfig::Iroh(iroh) = &mut self.config.transport {
                if iroh.secret_key_path.is_none() {
                    iroh.secret_key_path =
                        Some(PathBuf::from(format!("{}.iroh.key", path.display())));
                }
            }

            tracing::info!(
                storage_backend = "regolith",
                data_path = %path.display(),
                "embedded node starting"
            );

            // `Options::embedded` sizes the engine for a 1-4 MiB working
            // set, which is what an embedded node has to live inside.
            let opts =
                self.with_at_rest_keys(storage::RegolithStoreOptions::embedded().placed_on(&pool))?;
            let store =
                storage::RegolithStore::open_with_options(&path, opts).map_err(|source| {
                    Error::StoreOpen {
                        path: path.clone(),
                        source,
                    }
                })?;

            (EmbeddedStore::Regolith(store), Persistence::Persistent)
        } else {
            tracing::info!(
                storage_backend = "memory",
                "embedded node starting (ephemeral, no data_path)"
            );
            (
                EmbeddedStore::Regolith(
                    storage::RegolithStore::open_with_options(
                        "memory",
                        self.with_at_rest_keys(
                            storage::RegolithStoreOptions::memory().placed_on(&pool),
                        )?,
                    )
                    .map_err(Error::MemoryStoreOpen)?,
                ),
                Persistence::Memory,
            )
        };

        let store = Arc::new(store);

        self.config.persistence = persistence;
        #[cfg(feature = "iroh")]
        check_access_hooks(&self.config, self.access_hooks.as_ref())?;
        #[cfg(feature = "iroh")]
        return build_node(store, self.config, self.access_hooks, pool).await;
        #[cfg(not(feature = "iroh"))]
        build_node(store, self.config, None, pool).await
    }
}

pub struct ShutdownHandle {
    #[cfg(any(feature = "libp2p", feature = "iroh"))]
    inner: ShutdownKind,
}

#[cfg(any(feature = "libp2p", feature = "iroh"))]
enum ShutdownKind {
    #[cfg(feature = "libp2p")]
    Libp2p {
        handle: Box<p2p::P2PHostHandle>,
        coordinator: p2p::sync::SyncShutdownHandle,
        tasks: Box<SegQueue<edge::Service>>,
        /// The retry sweep, on the network edge.
        retry_loop: Box<SegQueue<edge::Task<()>>>,
        /// The push failure recorder, on the worker pool.
        recorder: Box<SegQueue<workers::Background>>,
    },
    #[cfg(feature = "iroh")]
    Iroh(defra_p2p_adapter::IrohPeerShutdown),
}

impl ShutdownHandle {
    #[cfg(feature = "libp2p")]
    pub(crate) fn libp2p(
        handle: p2p::P2PHostHandle,
        coordinator: p2p::sync::SyncShutdownHandle,
        tasks: Vec<edge::Service>,
        retry_loop: edge::Task<()>,
        recorder: workers::Background,
    ) -> Self {
        Self {
            inner: ShutdownKind::Libp2p {
                handle: Box::new(handle),
                coordinator,
                tasks: queued(tasks),
                retry_loop: queued([retry_loop]),
                recorder: queued([recorder]),
            },
        }
    }

    #[cfg(feature = "iroh")]
    pub(crate) fn iroh(peer: defra_p2p_adapter::IrohPeerShutdown) -> Self {
        Self {
            inner: ShutdownKind::Iroh(peer),
        }
    }

    pub async fn shutdown(&self) {
        #[cfg(any(feature = "libp2p", feature = "iroh"))]
        match &self.inner {
            #[cfg(feature = "libp2p")]
            ShutdownKind::Libp2p {
                handle,
                coordinator,
                tasks,
                retry_loop,
                recorder,
            } => {
                coordinator.shutdown().await;
                abort_and_join(retry_loop, edge::Task::abort).await;
                abort_and_join(recorder, workers::Background::abort).await;
                stop_services(tasks).await;
                let _ = handle.shutdown().await;
            }
            #[cfg(feature = "iroh")]
            ShutdownKind::Iroh(peer) => peer.shutdown().await,
        }

        defra_core::signing::clear_identity_store();
    }
}

/// `tasks` in a queue a shutdown through `&self` can take them from.
#[cfg(feature = "libp2p")]
fn queued<T: 'static>(tasks: impl IntoIterator<Item = T>) -> Box<SegQueue<T>> {
    let pending = SegQueue::new();
    tasks.into_iter().for_each(|task| pending.push(task));
    Box::new(pending)
}

#[cfg(feature = "libp2p")]
async fn stop_services(services: &SegQueue<edge::Service>) {
    let mut stopping = Vec::with_capacity(services.len());
    while let Some(service) = services.pop() {
        service.stop();
        stopping.push(service);
    }
    for service in stopping {
        service.stopped().await;
    }
}

#[cfg(feature = "libp2p")]
async fn abort_and_join<T: std::future::Future + 'static>(tasks: &SegQueue<T>, abort: impl Fn(&T)) {
    let mut aborted = Vec::with_capacity(tasks.len());
    while let Some(task) = tasks.pop() {
        abort(&task);
        aborted.push(task);
    }
    for task in aborted {
        let _ = task.await;
    }
}

/// The worker pool a node's data path runs on: `config` (the pool's
/// defaults when unset), with the network runtime's timer running its
/// stuck-poll monitor when the node has one.
pub fn start_node_pool(
    config: Option<workers::Config>,
    edge: Option<&edge::Edge>,
) -> Result<workers::Pool> {
    storage::start_pool(
        config.unwrap_or_default(),
        edge.map(edge::Edge::pool_monitor),
    )
    .map_err(Error::PoolStart)
}

/// Decides the network runtime once, before anything holds it: the one the
/// config names, or the process's own ([`edge::on_demand`]), started on
/// first use, when the node has a transport and the config names none. The
/// pool's stuck-poll monitor, the database's network work, the ACP and KMS
/// clients and the transport then read the same edge from the config. A
/// node without networking needs none.
fn resolve_edge(config: &mut EmbeddedNodeConfig) -> Result<()> {
    if config.edge.is_none() && !matches!(config.transport, TransportConfig::None) {
        config.edge = Some(edge::on_demand().map_err(Error::NetworkStart)?.clone());
    }
    Ok(())
}

/// A node over a store the application opened, on a worker pool of the
/// pool's default configuration. The store's own threads are not placed by
/// the pool: [`NodeBuilder`] opens the store on the pool and places them.
pub async fn build_with_store<S>(
    store: Arc<S>,
    mut config: EmbeddedNodeConfig,
) -> Result<EmbeddedNode<S>>
where
    S: storage::corekv::Store + 'static,
{
    resolve_edge(&mut config)?;
    let pool = start_node_pool(None, config.edge.as_ref())?;
    build_node(store, config, None, pool).await
}

/// A node over a store the application opened on `pool`
/// ([`storage::RegolithStoreOptions::placed_on`]), whose data path runs on
/// it. Start the pool with [`start_node_pool`].
pub async fn build_with_store_on_pool<S>(
    store: Arc<S>,
    mut config: EmbeddedNodeConfig,
    pool: workers::Pool,
) -> Result<EmbeddedNode<S>>
where
    S: storage::corekv::Store + 'static,
{
    resolve_edge(&mut config)?;
    build_node(store, config, None, pool).await
}

/// Access hooks need the iroh transport.
#[cfg(feature = "iroh")]
fn check_access_hooks(
    config: &EmbeddedNodeConfig,
    hooks: Option<&crate::AccessHooks>,
) -> Result<()> {
    if hooks.is_some() && !matches!(config.transport, TransportConfig::Iroh(_)) {
        return Err(Error::AccessHooksNeedIroh);
    }
    Ok(())
}

/// Build an iroh node whose access control the app supplies. Fails when
/// `hooks` is set and the transport is not iroh.
#[cfg(feature = "iroh")]
pub async fn build_with_store_and_access_hooks<S>(
    store: Arc<S>,
    mut config: EmbeddedNodeConfig,
    hooks: Option<crate::AccessHooks>,
) -> Result<EmbeddedNode<S>>
where
    S: storage::corekv::Store + 'static,
{
    check_access_hooks(&config, hooks.as_ref())?;
    resolve_edge(&mut config)?;
    let pool = start_node_pool(None, config.edge.as_ref())?;
    build_node(store, config, hooks, pool).await
}

#[cfg(feature = "iroh")]
type NodeAccessHooks = Option<crate::AccessHooks>;
#[cfg(not(feature = "iroh"))]
type NodeAccessHooks = Option<std::convert::Infallible>;

async fn build_node<S>(
    store: Arc<S>,
    config: EmbeddedNodeConfig,
    access_hooks: NodeAccessHooks,
    pool: workers::Pool,
) -> Result<EmbeddedNode<S>>
where
    S: storage::corekv::Store + 'static,
{
    let event_bus: Arc<dyn events::Bus> = Arc::new(events::ChannelBus::default());

    let (raw_identity, node_identity_did) = create_node_identity(&config.signing)?;
    let raw_identity = raw_identity.map(Arc::new);
    let mut db_options = db::DbOptions::default().with_pool(pool);
    if let Some(identity) = raw_identity.as_ref() {
        db_options = db_options.with_node_identity_arc(identity.clone());
    }

    let mut database = db::DB::open_from_arc_with_options(store.clone(), db_options)
        .await
        .map_err(Error::DbOpen)?;
    database.set_event_bus(event_bus.clone());
    if let Some(edge) = config.edge.as_ref() {
        database.set_network(edge.clone());
    }
    #[cfg(feature = "iroh")]
    if let Some(hooks) = &access_hooks {
        hooks.install(&database);
    }
    let database = Arc::new(database);
    // The vector sweep rebuilds HNSW graphs and merges DiskANN pending
    // inserts; without it an embedded DiskANN index never merges.
    let background_tasks = Arc::new(BackgroundTasks::new([
        database.clone().start_downsample_task(),
        database.clone().start_vector_rebuild_task(),
    ]));

    #[cfg(any(feature = "libp2p", feature = "iroh"))]
    let sync_config = SyncConfig {
        max_concurrent_dag_fetches: config
            .max_concurrent_dag_fetches
            .unwrap_or(p2p::sync::DEFAULT_MAX_CONCURRENT_DAG_FETCHES),
        max_concurrent_push_tasks: config
            .max_concurrent_push_tasks
            .unwrap_or(p2p::sync::DEFAULT_MAX_CONCURRENT_PUSH_TASKS),
        max_doc_sync_request_doc_ids: config
            .max_doc_sync_request_doc_ids
            .unwrap_or(p2p::sync::DEFAULT_MAX_DOC_SYNC_REQUEST_DOC_IDS),
        rate_limit_burst: config
            .rate_limit_burst
            .unwrap_or(p2p::sync::DEFAULT_RATE_LIMIT_BURST),
        rate_limit_rate: config
            .rate_limit_rate
            .unwrap_or(p2p::sync::DEFAULT_RATE_LIMIT_RATE),
        ..Default::default()
    };

    let acp_setup = create_document_acp(
        store.clone(),
        config.persistence,
        &config.document_acp,
        config.edge.as_ref(),
    )
    .await?;
    let document_acp = acp_setup.document_acp;
    let local_zanzibar_store = acp_setup.local_zanzibar_store;
    #[cfg(feature = "vera")]
    let vera_acp = acp_setup.vera_acp;
    #[cfg(feature = "vera")]
    if let Some(start_redrive) = acp_setup.redrive {
        background_tasks.push(start_redrive(database.pool()));
    }
    #[cfg(all(feature = "vera", any(feature = "libp2p", feature = "iroh")))]
    let strict_replicated_doc_access = vera_acp.is_some();
    #[cfg(all(not(feature = "vera"), any(feature = "libp2p", feature = "iroh")))]
    let strict_replicated_doc_access = false;

    let p2p_setup: Result<Option<crate::node_p2p::P2PSetup>> =
        match (&config.transport, config.edge.as_ref()) {
            #[cfg(feature = "libp2p")]
            (TransportConfig::Libp2p(libp2p), Some(edge)) => crate::node_p2p::setup_libp2p(
                store.clone(),
                database.clone(),
                event_bus.clone(),
                libp2p,
                sync_config.clone(),
                config.peer_keyring.as_ref(),
                edge,
            )
            .await
            .map(Some),
            #[cfg(feature = "iroh")]
            (TransportConfig::Iroh(iroh), Some(edge)) => crate::node_p2p::setup_iroh(
                store.clone(),
                database.clone(),
                event_bus.clone(),
                iroh,
                sync_config.clone(),
                raw_identity.clone(),
                document_acp.clone(),
                strict_replicated_doc_access,
                config.peer_keyring.as_ref(),
                edge,
            )
            .await
            .map(Some),
            (TransportConfig::None, _) => Ok(None),
            // A transport always has a runtime here (`resolve_edge`).
            #[cfg(any(feature = "libp2p", feature = "iroh"))]
            (_, None) => Ok(None),
        };
    let mut p2p_setup = match p2p_setup {
        Ok(setup) => setup,
        Err(error) => {
            background_tasks.shutdown().await;
            if let Err(close_error) = database.close().await {
                tracing::warn!(%close_error, "failed to close database after P2P setup error");
            }
            return Err(error);
        }
    };

    let nac_manager = create_nac_manager(store.clone(), config.persistence).await?;

    // Wire the NAC manager into the DB so DB-layer `check_node_access` calls go
    // live. First-call-wins via the DB's OnceCell setter. Covers both the
    // embedded node and the FFI node (which also builds via `build_with_store`).
    database.set_nac_manager(nac_manager.clone());

    if let Some(ref mut setup) = p2p_setup {
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
    }

    // Build the KMS once the NAC manager exists (PR #4778 ordering: the policy
    // needs document ACP, created before P2P, and NAC, which initializes here).
    let kms: Arc<dyn kms::KmsService> = {
        // Blockstore-backed KeyStore (mirrors Go's internal/kms/enc_store.go):
        // the KMS serves DEKs for ANY encrypted write by reading/writing the
        // node's durable encstore→blockstore, not a RAM-only map. The DB owns
        // the blockstore Arc (set_kms_blockstore) so the adapter can hold a Weak
        // and avoid the lock-pinning cycle (#976) while sharing the block cache.
        let kms_blockstore = database.set_kms_blockstore(Arc::new(
            blockstore::DefraBlockstore::new(store.clone(), true)
                .with_max_txn_retries(database.options().max_txn_retries())
                .placed_on(database.pool()),
        ));
        let enc_block_store: Arc<dyn kms::EncBlockStore> =
            Arc::new(db::DbEncBlockStore::new(database.clone(), kms_blockstore));
        let store: Arc<dyn kms::KeyStore> = Arc::new(kms::BlockstoreKeyStore::new(enc_block_store));
        let doc_lookup: Arc<dyn kms::DocCollectionLookup> =
            Arc::new(db::DbDocCollectionLookup::new(database.clone()));
        let policy = Arc::new(kms::NacDacPolicy::new(document_acp.clone(), doc_lookup));
        policy.set_node_acp(Arc::new(db::DbNodeAcpRead::new(nac_manager.clone())));

        // Node identity used for cross-peer DEK requests. Anonymous nodes use
        // a stable placeholder DID.
        let node_did = database.node_did().unwrap_or_else(|| {
            identity::Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")
                .expect("static anonymous DID parses")
        });

        let transports: Vec<Arc<dyn kms::KeyTransport>> = match p2p_setup.as_ref() {
            Some(setup) => vec![setup.kms_transport.clone()],
            None => vec![],
        };

        Arc::new(kms::DefraKms::with_inputs(
            store,
            transports,
            policy as Arc<dyn kms::AccessPolicy>,
            Arc::new(db::DbBlockDocIDResolver::new(database.clone())),
            node_did,
            config
                .edge
                .as_ref()
                .map(kms::KmsInputs::on)
                .unwrap_or_default(),
        ))
    };

    // Bind this node's transport peer id into the KMS so served ECIES
    // replies carry the correct AAD peer id (Go's `makeAssociatedData`).
    if let Some(ref setup) = p2p_setup {
        kms.set_local_peer_id(setup.local_peer_id.clone());
    }

    // Wire the KMS into the P2P transport (serve handler) + merge handler.
    if let Some(ref mut setup) = p2p_setup {
        // Install the serve handler. Use a Weak ref to break the
        // transport↔kms Arc cycle (transport holds handler → kms →
        // transports → transport).
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
    database.set_kms(kms.clone());

    let fetcher = db::LensedAutoCommitFetcher::new(database.clone());
    let collection_provider: Arc<dyn query::CollectionProvider> =
        db::DbCollectionProvider::new_arc(database.clone());
    let txn_broadcaster = p2p_setup
        .as_ref()
        .map(|setup| setup.txn_broadcaster.clone());
    let txn_registry = Arc::new(match txn_broadcaster {
        Some(b) => db::DbTransactionRegistry::with_broadcaster(database.clone(), b),
        None => db::DbTransactionRegistry::new(database.clone()),
    });

    let mut query_runner = query::QueryRunner::with_arc_registry_and_provider(
        fetcher,
        collection_provider,
        txn_registry.clone(),
    )
    .with_mutator(
        p2p_setup
            .as_ref()
            .map(|setup| setup.mutator.clone())
            .unwrap_or_else(|| Arc::new(db::AutoCommitMutator::new(database.clone()))),
    )
    .with_collection_truncator(db::DbCollectionTruncator::new_arc(database.clone()))
    .with_acp(document_acp.clone())
    .with_lens_store(database.lens_store().clone())
    .with_query_limits(config.query_limits);

    // Wire the SE remote query transport so this embedded node can act as an SE
    // query OWNER, fanning encrypted_<Collection> queries to replicators (#976).
    if let Some(se_transport) = p2p_setup
        .as_ref()
        .and_then(|setup| setup.se_transport.clone())
    {
        query_runner = query_runner.with_se_transport(se_transport);
    }

    #[cfg(feature = "iroh")]
    if let Some(hooks) = &access_hooks {
        query_runner = hooks.apply(query_runner);
    }
    #[cfg(not(feature = "iroh"))]
    let _ = access_hooks;

    // GraphQL requests run on the database's worker pool, routed there once
    // at the boundary.
    let query_runner = query::pooled::PooledExecutor::wrap(Arc::new(query_runner), database.pool());

    Ok(EmbeddedNode {
        database,
        background_tasks,
        txn_registry,
        query_runner,
        nac_manager,
        document_acp,
        local_zanzibar_store,
        event_bus,
        node_identity_did,
        #[cfg(feature = "vera")]
        vera_acp,
        query_limits: config.query_limits,
        p2p: p2p_setup.map(|setup| setup.system),
        shutdown_started: AtomicBool::new(false),
        shutdown_finished: AtomicBool::new(false),
        shutdown_notify: Notify::new(),
    })
}

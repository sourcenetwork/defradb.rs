/// Database struct for DefraDB matching Go's internal/db/db.go.
///
/// The DB struct is the main entry point for DefraDB operations.
/// It manages the root store, creates transactions, and provides
/// access to collections.
use crate::error::{Error, Result};
pub use crate::search::EmbeddingClientConfig;
use crate::txn::DbTxn;
use crate::NacManagerApi;
use cid::Cid;
use datastore::BasicTxn;
use events::Bus;
use identity::{Identity, RawIdentity};
use kovan_map::HopscotchMap;
use lens::TransformStore;
use rapidhash::fast::RandomState;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use storage::corekv::{Key, Store};

pub mod action;
pub(crate) mod dump;
pub(crate) mod spawn;
pub mod storage_stats;

pub use storage::corekv::DEFAULT_MAX_TXN_RETRIES;

/// Database options.
#[derive(Clone, Default)]
pub struct DbOptions {
    /// Maximum number of transaction retries.
    pub max_txn_retries: Option<u32>,
    /// Chunk size for large values in the blockstore.
    pub chunk_size: Option<usize>,
    /// Node identity for this database instance.
    ///
    /// The node identity is used for:
    /// - Signing documents and blocks
    /// - Authenticating with the ACP (Access Control Policy) system
    /// - Identifying this node in P2P interactions
    pub node_identity: Option<Arc<RawIdentity>>,
    /// Fallback OpenAI-compatible embedding base URL.
    pub embedding_url: String,
    /// Fallback embedding model name.
    pub embedding_model: String,
    /// Resolved embedding API key value.
    pub embedding_api_key: String,
    /// The worker pool this database's work runs on: its periodic and
    /// detached jobs, and the pool [`DB::close`] stops before the store
    /// closes. `None` (a test, a one-shot tool) runs them on the process's
    /// default pool.
    pub pool: Option<workers::Pool>,
}

impl std::fmt::Debug for DbOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbOptions")
            .field("max_txn_retries", &self.max_txn_retries)
            .field("chunk_size", &self.chunk_size)
            .field(
                "node_identity",
                &self.node_identity.as_ref().map(|id| {
                    id.did()
                        .map(|d| d.to_string())
                        .unwrap_or_else(|_| "<invalid>".to_string())
                }),
            )
            .field("embedding_url", &self.embedding_url)
            .field("embedding_model", &self.embedding_model)
            .field(
                "embedding_api_key_configured",
                &!self.embedding_api_key.is_empty(),
            )
            .field("pool", &self.pool)
            .finish()
    }
}

impl DbOptions {
    /// Creates new database options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the node identity for this database.
    pub fn with_node_identity(mut self, identity: RawIdentity) -> Self {
        self.node_identity = Some(Arc::new(identity));
        self
    }

    /// Sets the node identity from an Arc for this database.
    pub fn with_node_identity_arc(mut self, identity: Arc<RawIdentity>) -> Self {
        self.node_identity = Some(identity);
        self
    }

    /// Sets the maximum number of transaction retries.
    pub fn with_max_txn_retries(mut self, retries: u32) -> Self {
        self.max_txn_retries = Some(retries);
        self
    }

    /// Returns the maximum number of transaction retries.
    pub fn max_txn_retries(&self) -> u32 {
        self.max_txn_retries.unwrap_or(DEFAULT_MAX_TXN_RETRIES)
    }

    /// The engine retry policy every write of this database runs under.
    pub fn retry_options(&self) -> storage::corekv::RetryOptions {
        storage::corekv::engine_retry_options(self.max_txn_retries())
    }

    /// Sets the chunk size for large values.
    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = Some(size);
        self
    }

    /// Sets the fallback embedding base URL.
    pub fn with_embedding_url(mut self, url: impl Into<String>) -> Self {
        self.embedding_url = url.into();
        self
    }

    /// Sets the fallback embedding model name.
    pub fn with_embedding_model(mut self, model: impl Into<String>) -> Self {
        self.embedding_model = model.into();
        self
    }

    /// Sets the resolved embedding API key value.
    pub fn with_embedding_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.embedding_api_key = api_key.into();
        self
    }

    /// Runs the database's work on `pool`, the pool its store was placed on.
    pub fn with_pool(mut self, pool: workers::Pool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Returns the embedding client configuration for this database.
    pub fn embedding_config(&self) -> EmbeddingClientConfig {
        EmbeddingClientConfig::new()
            .with_url(self.embedding_url.clone())
            .with_model(self.embedding_model.clone())
            .with_api_key(self.embedding_api_key.clone())
    }
}

/// The main DefraDB database struct.
///
/// This matches Go's DB struct in internal/db/db.go.
pub struct DB<S: Store> {
    /// The underlying store.
    store: Arc<S>,
    /// Options for this database instance.
    options: DbOptions,
    /// Counter for generating unique transaction IDs.
    txn_id_counter: AtomicU64,
    /// Generation of the committed migration graph.
    ///
    /// Lensed fetchers include this in their history-cache validity checks so
    /// registering another transform invalidates both positive and negative
    /// cached migration contexts, even when the active schema version does not
    /// change.
    migration_generation: AtomicU64,
    /// Whether the database has been closed.
    closed: AtomicBool,
    /// Commits whose caller stopped waiting, fenced until they land: every
    /// new transaction waits for them.
    commit_fences: Arc<crate::txn::fence::CommitFences>,
    /// In-memory collection cache (name -> Collection).
    pub(crate) collections: Arc<crate::collection::epoch::EpochedCollections>,
    /// PQ codes of resident-codes DiskANN indexes, shared by every search.
    vector_codes: Arc<crate::index::vector::engine::diskann::CodeCaches>,
    /// Event bus for subscription notifications.
    event_bus: Option<Arc<dyn Bus>>,
    /// Lens transform store for schema migrations.
    pub lens_store: Arc<dyn TransformStore>,
    /// Pending migrations registered before their destination version exists.
    /// Maps dest_version_id -> (source_version_id, transform_id_string).
    pub(crate) pending_migrations: HopscotchMap<String, (String, String), RandomState>,
    /// Schema definition headstore: tracks latest CID and height per collection.
    /// Emulates Go's persistent headstore for CID computation during patching.
    /// Key: collection name, Value: (sorted heads as CIDs, max height)
    pub(crate) schema_heads: Arc<HopscotchMap<String, (Vec<Cid>, u64), RandomState>>,
    /// Collection IDs whose last local version has been deleted.
    ///
    /// Go's collection repository forbids these immediately, including for
    /// transactions that started before the deletion committed.
    pub(crate) forbidden_collection_ids: HopscotchMap<String, (), RandomState>,
    /// Optional KMS service. When set, the document write path generates
    /// encrypted-field DEKs through the KMS (which persists them in its
    /// KeyStore for cross-peer serving) instead of inline-and-blockstore.
    /// Set once at node startup via [`DB::set_kms`].
    document_acp: regolith::sync::OnceCell<Arc<dyn acp::DocumentACP>>,
    kms: regolith::sync::OnceCell<std::sync::Arc<dyn kms::KmsService>>,
    /// Remote signatures and embeddings, each behind its own service.
    remote_inputs: crate::remote_inputs::RemoteInputs,
    /// Where the database's P2P work (pushes, replays, retries) runs its
    /// network waits: the node's network runtime. Set once at startup.
    network: regolith::sync::OnceCell<p2p::network::Network>,
    /// Owning handle to the KMS durable blockstore. The KMS adapter
    /// (`DbEncBlockStore`) references this weakly to avoid the
    /// DB→KMS→`KeyStore`→adapter→blockstore→store Arc cycle that would pin the
    /// storage lock past node close (#976). Parking the owning `Arc` here means
    /// it shares the DB's lifetime — and its in-process block cache — and drops
    /// with the DB, releasing the lock. Set once at startup.
    kms_blockstore: regolith::sync::OnceCell<Arc<blockstore::DefraBlockstore<S>>>,
    /// Optional NAC manager. When set and enabled, node-level operations are
    /// gated through [`DB::check_node_access`]. Set once at node startup via
    /// [`DB::set_nac_manager`]. When unset, all `check_node_access` calls are
    /// no-ops (NAC not configured).
    nac_manager: regolith::sync::OnceCell<std::sync::Arc<dyn NacManagerApi>>,
    /// Collections an app has claimed and the validator governing their
    /// replicated composites. Set once, before replication starts.
    ///
    /// Held by value: every reader borrows it, and on wasm the validator is
    /// `?Send`, so wrapping it in an `Arc` would be an `Arc` over a value that
    /// is neither `Send` nor `Sync`.
    merge_governance: regolith::sync::OnceCell<crate::merge::governance::MergeGovernance>,
    /// Told about each composite a local write commits, so composites deferred
    /// awaiting what the write created are released.
    local_commit_release:
        regolith::sync::OnceCell<Arc<dyn crate::merge::governance::LocalCommitRelease>>,
    /// Process-local document short-ID range allocator.
    doc_short_id_allocator: crate::docid::map::DocShortIdAllocator<S>,
    /// Process-local claims for collection-wide actions.
    ///
    /// Persisted action statuses are observable lifecycle state, not locks: a
    /// process can exit while an action is in progress and leave that state
    /// behind. The registry provides cancellation-safe mutual exclusion for
    /// operations running in this database instance.
    pub active_actions: Arc<crate::database::action::ActionRegistry>,
}

impl<S: Store> DB<S> {
    /// Create a new database with the given store.
    ///
    /// This creates a DB with an empty collection cache. Use `open()` to
    /// load existing collections from the store.
    pub fn new(store: S) -> Result<Self> {
        Self::with_options(store, DbOptions::default())
    }

    /// Create a new database with the given store and options.
    ///
    /// This creates a DB with an empty collection cache. Use `open_with_options()`
    /// to load existing collections from the store.
    pub fn with_options(store: S, options: DbOptions) -> Result<Self> {
        let lens_store: Arc<dyn TransformStore> = Self::create_lens_store(options.pool.as_ref())?;
        let store = Arc::new(store);
        Ok(Self {
            doc_short_id_allocator: crate::docid::map::DocShortIdAllocator::new(store.clone()),
            store,
            options,
            txn_id_counter: AtomicU64::new(0),
            migration_generation: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            commit_fences: Arc::default(),
            collections: Arc::new(crate::collection::epoch::EpochedCollections::new(
                crate::collection::CollectionMap::default(),
            )),
            vector_codes: Arc::default(),
            event_bus: None,
            lens_store,
            pending_migrations: HopscotchMap::with_hasher(RandomState::default()),
            schema_heads: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
            forbidden_collection_ids: HopscotchMap::with_hasher(RandomState::default()),
            document_acp: regolith::sync::OnceCell::new(),
            kms: regolith::sync::OnceCell::new(),
            remote_inputs: Default::default(),
            network: regolith::sync::OnceCell::new(),
            kms_blockstore: regolith::sync::OnceCell::new(),
            nac_manager: regolith::sync::OnceCell::new(),
            merge_governance: regolith::sync::OnceCell::new(),
            local_commit_release: regolith::sync::OnceCell::new(),
            active_actions: Arc::new(crate::database::action::ActionRegistry::default()),
        })
    }

    /// Open a database and load existing collections from the store.
    pub async fn open(store: S) -> Result<Self>
    where
        S: 'static,
    {
        Self::open_with_options(store, DbOptions::default()).await
    }

    /// Open a database with options and load existing collections from the store.
    pub async fn open_with_options(store: S, options: DbOptions) -> Result<Self>
    where
        S: 'static,
    {
        let db = Self::with_options(store, options)?;
        db.finish_open().await?;
        Ok(db)
    }

    /// Create a new database from an Arc-wrapped store.
    ///
    /// Use this when you already have an `Arc<S>` and want to share
    /// the store between the database and other components (e.g., blockstore).
    ///
    /// Note: This creates a DB with an empty collection cache. Use `open_from_arc()`
    /// to load existing collections from the store.
    ///
    /// **Warning:** When multiple DB instances share a store via `from_arc()`,
    /// transaction IDs may collide if both instances create transactions concurrently.
    /// This is acceptable for read-heavy workloads but may cause issues with
    /// concurrent writes from multiple DB instances.
    pub fn from_arc(store: Arc<S>) -> Result<Self> {
        Self::from_arc_with_options(store, DbOptions::default())
    }

    /// Create a new database from an Arc-wrapped store with options.
    ///
    /// **Warning:** When multiple DB instances share a store via `from_arc()`,
    /// transaction IDs may collide if both instances create transactions concurrently.
    pub fn from_arc_with_options(store: Arc<S>, options: DbOptions) -> Result<Self> {
        let lens_store: Arc<dyn TransformStore> = Self::create_lens_store(options.pool.as_ref())?;
        Ok(Self {
            doc_short_id_allocator: crate::docid::map::DocShortIdAllocator::new(store.clone()),
            store,
            options,
            txn_id_counter: AtomicU64::new(0),
            migration_generation: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            commit_fences: Arc::default(),
            collections: Arc::new(crate::collection::epoch::EpochedCollections::new(
                crate::collection::CollectionMap::default(),
            )),
            vector_codes: Arc::default(),
            event_bus: None,
            lens_store,
            pending_migrations: HopscotchMap::with_hasher(RandomState::default()),
            schema_heads: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
            forbidden_collection_ids: HopscotchMap::with_hasher(RandomState::default()),
            document_acp: regolith::sync::OnceCell::new(),
            kms: regolith::sync::OnceCell::new(),
            remote_inputs: Default::default(),
            network: regolith::sync::OnceCell::new(),
            kms_blockstore: regolith::sync::OnceCell::new(),
            nac_manager: regolith::sync::OnceCell::new(),
            merge_governance: regolith::sync::OnceCell::new(),
            local_commit_release: regolith::sync::OnceCell::new(),
            active_actions: Arc::new(crate::database::action::ActionRegistry::default()),
        })
    }

    /// Open a database from an Arc-wrapped store and load existing collections.
    ///
    /// Use this when you already have an `Arc<S>` and want to share
    /// the store between the database and other components (e.g., blockstore),
    /// while also loading existing collections from the store.
    pub async fn open_from_arc(store: Arc<S>) -> Result<Self>
    where
        S: 'static,
    {
        Self::open_from_arc_with_options(store, DbOptions::default()).await
    }

    /// Open a database from an Arc-wrapped store with options and load existing collections.
    pub async fn open_from_arc_with_options(store: Arc<S>, options: DbOptions) -> Result<Self>
    where
        S: 'static,
    {
        let db = Self::from_arc_with_options(store, options)?;
        db.finish_open().await?;
        Ok(db)
    }

    /// The startup sequence every open path runs once the store is attached.
    ///
    /// There are two ways in, taking a store and taking an `Arc` of one, and
    /// they have to do the same work: a step added to only one of them runs for
    /// half the callers.
    async fn finish_open(&self) -> Result<()>
    where
        S: 'static,
    {
        self.load_collections().await?;
        self.initialize_migrations().await?;
        self.migrate_document_parts().await?;
        // Counters move into their document part, so they need the part IDs the step above gives
        // collections written before part IDs existed.
        self.maybe_migrate_counter_accumulators().await?;
        self.migrate_index_format().await?;
        self.resume_index_drops().await?;
        self.resume_index_backfills().await?;
        // Reindexes owed by schema changes a crash or a failure left unsettled.
        self.settle_owed_reindexes_after_commit().await;
        Ok(())
    }

    /// Set the event bus for subscription notifications.
    ///
    /// When an event bus is set, document mutations (create, update, delete)
    /// will emit update events that can be received by subscribers.
    pub fn set_event_bus(&mut self, bus: Arc<dyn Bus>) {
        self.event_bus = Some(bus);
    }

    /// Get a reference to the event bus, if configured.
    pub fn event_bus(&self) -> Option<&Arc<dyn Bus>> {
        self.event_bus.as_ref()
    }

    /// Allocate a globally unique document short ID.
    pub async fn next_doc_short_id(&self) -> Result<u64> {
        self.doc_short_id_allocator.next().await
    }

    /// Reserve a process-independent upper fence for index backfill.
    ///
    /// A DB handle's cached short-ID range is not a global high-water mark:
    /// another handle may already have reserved and used a later range. A
    /// single global allocation gives backfill a fence above every ID handed
    /// out before this call. The reserved ID is intentionally left unused.
    pub(crate) async fn reserve_doc_short_id_fence(&self) -> Result<u64> {
        storage::stores::Systemstore::new(self.store.clone())
            .allocate(
                &storage::keys::doc_id_index::DocShortIDSequenceKey::new().bytes(),
                1,
            )
            .await
            .map(|range| range.start)
            .map_err(Error::Storage)
    }

    /// Resolve a document short ID or allocate and stage a new mapping.
    pub async fn resolve_or_allocate_doc_short_id(
        &self,
        systemstore: &datastore::NamespaceView,
        collection_short_id: u32,
        doc_id: &str,
    ) -> Result<u64> {
        if let Some(short_id) =
            crate::docid::map::get_doc_short_id(systemstore, collection_short_id, doc_id).await?
        {
            return Ok(short_id);
        }

        let short_id = self.next_doc_short_id().await?;
        crate::docid::map::set_doc_id_mapping(systemstore, collection_short_id, short_id, doc_id)
            .await?;
        Ok(short_id)
    }

    /// The epoch of the committed collection set: advances on every swap of
    /// the process-wide collection cache, so an unchanged value proves the
    /// set of collections a query sees is unchanged. Introspection keys its
    /// built-schema cache on it.
    pub fn schema_epoch(&self) -> u64 {
        self.collections.epoch()
    }

    /// Return the generation of the committed migration graph.
    pub fn migration_generation(&self) -> u64 {
        self.migration_generation.load(Ordering::Acquire)
    }

    /// Invalidate migration-history caches after a migration commit.
    pub(crate) fn bump_migration_generation(&self) {
        self.migration_generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Install the node's document authorization provider before serving
    /// mutations. A non-transactional provider is gated by the registrations
    /// creates owe it (see [`acp::ExternalDocumentAcp`]) unless it already is.
    pub fn set_document_acp(&self, provider: Arc<dyn acp::DocumentACP>)
    where
        S: 'static,
    {
        let provider: Arc<dyn acp::DocumentACP> =
            if provider.supports_transactions() || provider.settles_owed_registrations() {
                provider
            } else {
                Arc::new(acp::ExternalDocumentAcp::new(
                    provider,
                    Arc::clone(&self.store),
                ))
            };
        let _ = self.document_acp.set(provider);
    }

    pub fn document_acp(&self) -> Option<&Arc<dyn acp::DocumentACP>> {
        self.document_acp.get()
    }

    /// Install the KMS service. First call wins (OnceCell); subsequent calls
    /// are silently discarded. Called once at node startup.
    /// True when no process-local action claim is held.
    pub fn has_no_active_actions(&self) -> bool {
        self.active_actions.is_empty()
    }

    /// Replace the transform store backing lens migrations.
    pub fn set_lens_store(&mut self, store: Arc<dyn TransformStore>) {
        self.lens_store =
            crate::definition::migration::committed_lenses::CommittedLensStore::wrap(store);
    }

    pub fn set_kms(&self, kms: std::sync::Arc<dyn kms::KmsService>) {
        let _ = self.kms.set(kms);
    }

    /// Get the KMS service, if one has been installed.
    pub fn kms(&self) -> Option<std::sync::Arc<dyn kms::KmsService>> {
        self.kms.get().cloned()
    }

    /// Runs the database's remote inputs (signatures, embeddings) and the
    /// network waits of its P2P work on the node's network runtime. Set once
    /// at startup, before the first write that needs one; without it
    /// signatures run in the caller's task and embeddings on the process's
    /// on-demand network runtime.
    pub fn set_network(&self, edge: edge::Edge) {
        let p2p = self
            .network
            .set(p2p::network::Network::on(edge.clone()))
            .is_ok();
        if !self.remote_inputs.set_runtime(inputs::Runtime::edge(edge)) || !p2p {
            tracing::warn!("the database's network runtime was already set; keeping the first");
        }
    }

    /// Where the database's P2P work runs its network waits: the runtime
    /// [`set_network`](Self::set_network) set, or without one the runtime of
    /// the task that waits.
    pub fn network(&self) -> p2p::network::Network {
        self.network.get().cloned().unwrap_or_default()
    }

    pub(crate) fn remote_inputs(&self) -> &crate::remote_inputs::RemoteInputs {
        &self.remote_inputs
    }

    /// Install the NAC manager. First call wins (OnceCell); subsequent calls
    /// are silently discarded. Called once at node startup. When unset, all
    /// `check_node_access` calls are no-ops (NAC not configured).
    pub fn set_nac_manager(&self, nac: std::sync::Arc<dyn NacManagerApi>) {
        let _ = self.nac_manager.set(nac);
    }

    /// Install app merge governance. First call wins; install it before any
    /// replication starts so no composite of a claimed collection merges
    /// ungoverned.
    pub fn set_merge_governance(&self, governance: crate::merge::governance::MergeGovernance) {
        let _ = self.merge_governance.set(governance);
    }

    pub fn merge_governance(&self) -> Option<&crate::merge::governance::MergeGovernance> {
        self.merge_governance.get()
    }

    /// Install the hook that releases deferred composites awaiting what a
    /// local write creates. First call wins.
    pub fn set_local_commit_release(
        &self,
        release: Arc<dyn crate::merge::governance::LocalCommitRelease>,
    ) {
        let _ = self.local_commit_release.set(release);
    }

    pub(crate) fn local_commit_release(
        &self,
    ) -> Option<&Arc<dyn crate::merge::governance::LocalCommitRelease>> {
        self.local_commit_release.get()
    }

    /// Get the NAC manager, if one has been installed.
    pub fn nac_manager(&self) -> Option<std::sync::Arc<dyn NacManagerApi>> {
        self.nac_manager.get().cloned()
    }

    /// Park the KMS durable blockstore on the DB so its owning `Arc` shares the
    /// DB's lifetime (and block cache) without forming a lock-pinning cycle
    /// (#976). First call wins. Returns the stored handle (the just-set one, or
    /// the existing one if already set) so the caller can build a `Weak` from
    /// the canonical instance.
    pub fn set_kms_blockstore(
        &self,
        blockstore: Arc<blockstore::DefraBlockstore<S>>,
    ) -> Arc<blockstore::DefraBlockstore<S>> {
        let _ = self.kms_blockstore.set(blockstore);
        self.kms_blockstore
            .get()
            .expect("kms_blockstore set above")
            .clone()
    }

    /// Where commits whose caller stopped waiting are fenced until they land.
    pub(crate) fn commit_fences(&self) -> &Arc<crate::txn::fence::CommitFences> {
        &self.commit_fences
    }

    /// Get the next transaction ID.
    fn next_txn_id(&self) -> u64 {
        self.txn_id_counter.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Create a new transaction.
    ///
    /// If `readonly` is true, the transaction cannot perform writes.
    /// Returns `Error::DatabaseClosed` if the database has been closed.
    pub async fn new_txn(&self, readonly: bool) -> Result<DbTxn<S>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(Error::DatabaseClosed);
        }
        // A commit whose caller stopped waiting may still be landing; what
        // begins now must see it.
        if self.commit_fences.any() {
            self.commit_fences.cleared().await;
        }
        let id = self.next_txn_id();
        let basic_txn = BasicTxn::new(&*self.store, id, readonly)
            .await
            .map_err(Error::Storage)?;
        Ok(self.new_db_txn(basic_txn))
    }

    /// Runs `read` against a read-only transaction lent to it, then discards
    /// the transaction. `read` gets the transaction by value and must drop
    /// every handle it made of it before it returns.
    pub(crate) async fn read_lent<T, F, Fut>(&self, read: F) -> Result<T>
    where
        F: FnOnce(crate::txn::LentTxn<S>) -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let txn = Arc::new(self.new_txn(true).await?);
        let output = read(Arc::clone(&txn)).await;
        match crate::txn::reclaim(txn).map(crate::txn::DbTxn::discard) {
            Ok(Ok(())) => {}
            Ok(Err(error)) | Err(error) => {
                tracing::warn!(%error, "failed to discard a read-only transaction");
            }
        }
        Ok(output)
    }

    /// Wraps a BasicTxn created by a storage-owned retry attempt while keeping
    /// database-scoped caches shared with ordinary transactions.
    pub(crate) fn new_db_txn(&self, basic_txn: BasicTxn) -> DbTxn<S> {
        DbTxn::of_database(basic_txn, self.vector_codes.clone())
            .with_shared_collections(Arc::clone(&self.collections))
    }

    pub(crate) fn vector_codes(&self) -> &crate::index::vector::engine::diskann::CodeCaches {
        &self.vector_codes
    }

    /// Execute a function within a transaction.
    ///
    /// If the function returns Ok, the transaction is committed.
    /// If the function returns Err, the transaction is discarded.
    pub async fn with_txn<F, T>(&self, readonly: bool, f: F) -> Result<T>
    where
        F: FnOnce(&DbTxn<S>) -> Result<T>,
    {
        let txn = self.new_txn(readonly).await?;
        let result = f(&txn);
        match result {
            Ok(value) => {
                txn.commit().await?;
                Ok(value)
            }
            Err(e) => {
                // Discard and log if it fails - return original error
                if let Err(discard_err) = txn.discard() {
                    tracing::warn!(
                        error = %discard_err,
                        original_error = %e,
                        "Transaction discard failed after operation error"
                    );
                }
                Err(e)
            }
        }
    }

    /// Execute an async function within a transaction.
    ///
    /// If the function returns Ok, the transaction is committed.
    /// If the function returns Err, the transaction is discarded.
    pub async fn with_txn_async<F, Fut, T>(&self, readonly: bool, f: F) -> Result<T>
    where
        F: FnOnce(DbTxn<S>) -> Fut,
        Fut: std::future::Future<Output = (DbTxn<S>, Result<T>)>,
    {
        let txn = self.new_txn(readonly).await?;
        let (txn, result) = f(txn).await;
        match result {
            Ok(value) => {
                txn.commit().await?;
                Ok(value)
            }
            Err(e) => {
                // Discard and log if it fails - return original error
                if let Err(discard_err) = txn.discard() {
                    tracing::warn!(
                        error = %discard_err,
                        original_error = %e,
                        "Transaction discard failed after async operation error"
                    );
                }
                Err(e)
            }
        }
    }

    /// Close the database.
    ///
    /// After closing, any attempt to create new transactions will return
    /// `Error::DatabaseClosed`. Matches Go's close-guard behavior (PR #4435).
    /// Closes the database, in the one shutdown order: new transactions are
    /// refused, the pool finishes the work it admitted and stops (within its
    /// shutdown deadline), then the store closes. Called on a worker, the
    /// pool is left to its owner: a worker cannot wait for itself to stop.
    pub async fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        if let Some(pool) = &self.options.pool {
            match pool.stop().await {
                Ok(()) | Err(workers::Error::OnWorker) => {}
                Err(error) => {
                    tracing::warn!(%error, "the worker pool did not stop cleanly before the store closed");
                }
            }
        }
        self.store.close().await.map_err(Error::Storage)
    }

    /// The pool this database's work runs on, when it was given one.
    pub fn pool(&self) -> Option<&workers::Pool> {
        self.options.pool.as_ref()
    }

    /// The pool this database's pool work runs on: its own, else the
    /// process's default pool ([`workers::default_pool`]).
    pub fn pool_or_default(&self) -> std::result::Result<workers::Pool, workers::Error> {
        self.options
            .pool
            .clone()
            .map_or_else(workers::default_pool, Ok)
    }

    /// Returns true if the database has been closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Get a reference to the underlying store.
    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    /// Get the database options.
    pub fn options(&self) -> &DbOptions {
        &self.options
    }

    /// The peerstore over this database's store, running its conflicts under
    /// the database's retry policy.
    pub fn peerstore(&self) -> storage::stores::Peerstore<S> {
        storage::stores::Peerstore::new(self.store.clone())
            .with_max_txn_retries(self.options.max_txn_retries())
            .placed_on(self.pool())
    }

    /// Returns the node identity, if one was configured.
    ///
    /// The node identity is used for:
    /// - Signing documents and blocks
    /// - Authenticating with the ACP (Access Control Policy) system
    /// - Identifying this node in P2P interactions
    pub fn node_identity(&self) -> Option<Arc<RawIdentity>> {
        self.options.node_identity.clone()
    }

    /// Returns true if this database has a configured node identity.
    pub fn has_node_identity(&self) -> bool {
        self.options.node_identity.is_some()
    }

    /// Returns the node identity's DID, if configured and derivable.
    ///
    /// Used by ACP checks to apply the node-identity full-access shortcut.
    /// Returns `None` if the node identity is not configured or its public
    /// key cannot be converted into a DID.
    pub fn node_did(&self) -> Option<identity::Did> {
        self.options
            .node_identity
            .as_ref()
            .and_then(|id| id.did().ok())
    }

    /// The lens transform store this database runs its migrations on.
    /// The database's lens store: its epoch ticks run on `pool`'s deadline
    /// queue, or on a thread of their own without one.
    fn create_lens_store(pool: Option<&workers::Pool>) -> Result<Arc<dyn TransformStore>> {
        #[cfg(not(target_arch = "wasm32"))]
        let store = match pool {
            Some(pool) => lens::default_store_on(pool),
            None => lens::default_store(),
        };
        // A browser build compiles no lens runtime: nothing ticks.
        #[cfg(target_arch = "wasm32")]
        let store = {
            let _ = pool;
            lens::default_store()
        };
        store
            .map(crate::definition::migration::committed_lenses::CommittedLensStore::wrap)
            .map_err(|e| Error::Lens(format!("failed to create lens transform store: {}", e)))
    }

    /// Get the current transaction ID counter value.
    pub fn current_txn_id(&self) -> u64 {
        self.txn_id_counter.load(Ordering::SeqCst)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod close_tests;

#[cfg(test)]
mod vector_cache_tests {
    use super::*;

    #[test]
    fn retry_transactions_share_database_vector_code_caches() {
        workers::block_on(async {
            let db = DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap();
            let ordinary = db.new_txn(true).await.unwrap();
            let basic = BasicTxn::new(&*db.store, db.next_txn_id(), true)
                .await
                .unwrap();
            let retry = db.new_db_txn(basic);

            let ordinary_cache = ordinary.vector_codes().get(17, 29);
            let retry_cache = retry.vector_codes().get(17, 29);
            assert!(Arc::ptr_eq(&ordinary_cache, &retry_cache));
        })
        .unwrap()
    }

    /// A transaction of a database is built with the database's caches, so
    /// it builds none of its own: each would allocate a lock-free map only to
    /// drop it, on every transaction.
    #[test]
    fn a_database_transaction_builds_no_code_caches() {
        workers::block_on(async {
            use crate::index::vector::engine::diskann::CODE_CACHES_BUILT;

            let db = DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap();
            let built = CODE_CACHES_BUILT.with(std::cell::Cell::get);
            for readonly in [true, false] {
                db.new_txn(readonly).await.unwrap().discard().unwrap();
            }
            assert_eq!(CODE_CACHES_BUILT.with(std::cell::Cell::get), built);
        })
        .unwrap()
    }
}

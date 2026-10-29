//! Reusable embedded DefraDB node builder.
//!
//! Wraps defradb.rs library crates behind a clean builder API so that
//! downstream binaries can embed a DefraDB instance without duplicating
//! wiring code.
//!
//! P2P runs over Iroh/QUIC, or over libp2p when the `libp2p` feature is on.
//!
//! ## Cargo features
//!
//! - `native` — native host (event channel). Default-on.
//! - `vera` — on-chain document ACP. Default-on. Omit for local-only ACP.
//! - `wasmtime-runtime` — Lens WASM execution. Default-on. Without it,
//!   [`Node::set_migration`] returns an explicit error.
//! - `p2p` — Iroh/QUIC replication. Implies `native`. Does **not** compile libp2p.
//!   `iroh` is the same feature under its former name.
//! - `libp2p` — libp2p replication. Implies `native`. Off by default, so the
//!   default node's dependency graph has no libp2p.
//! - `system-keyring` — the OS keyring backend for the peer keyring.
//! - `http` — GraphQL HTTP server.
//! - `otel` — OpenTelemetry exporter.

#[cfg(feature = "p2p")]
mod access_hooks;
mod acp_ops;
mod assemble;
#[cfg(not(target_arch = "wasm32"))]
mod background_tasks;
mod benchmark_data_gen;
mod benchmark_queries;
mod benchmark_stats;
#[doc(hidden)]
pub mod benchmark_support;
pub mod coding_search;
pub mod config;
#[cfg(test)]
mod dac_api_tests;
mod db_impls;
pub mod dense_search;
mod error;
#[cfg(feature = "libp2p")]
mod libp2p_recovery;
#[cfg(feature = "libp2p")]
mod libp2p_runtime;
#[cfg(feature = "libp2p")]
mod libp2p_tasks;
#[cfg(feature = "managed-p2p")]
mod managed_p2p;
mod network;
mod node_acp;
mod node_builder;
mod node_identity;
#[cfg(test)]
mod node_query_identity_tests;
#[cfg(test)]
mod on_pool_tests;
mod ops;
#[cfg(any(feature = "p2p", feature = "libp2p"))]
mod p2p_runtime;
#[cfg(any(feature = "p2p", feature = "libp2p"))]
mod peer_key;
mod peer_keyring;
pub mod search_chunks;
mod signed_query;
mod transaction;
pub mod version;

use std::sync::Arc;
use std::time::Duration;

use defra_core::RequestContext;

#[cfg(feature = "p2p")]
pub use access_hooks::AccessHooks;
/// Re-exported so callers can name `Arc<dyn DocumentACP>` (the type
/// [`Node::document_acp`] returns) without depending on `acp` directly.
pub use acp::{DocumentACP, DocumentPermission, Identity as AcpIdentity};
#[cfg(not(target_arch = "wasm32"))]
pub use background_tasks::BackgroundTasks;
pub use coding_search::{
    CodingHybridSearchHit, CodingHybridSearchRequest, CodingHybridSearchResponse,
    CodingSearchTarget,
};
#[cfg(feature = "http")]
pub use config::HttpConfig;
#[cfg(feature = "libp2p")]
pub use config::Libp2pConfig;
#[cfg(any(feature = "p2p", feature = "libp2p"))]
pub use config::SyncTuning;
#[cfg(feature = "vera")]
pub use config::VeraConfig;
pub use config::{
    DocumentAcpConfig, NodeConfig, NodeSigning, Persistence, SigningKey, TransportConfig,
};
#[cfg(feature = "p2p")]
pub use config::{P2PConfig, PeerKeySource};
#[cfg(feature = "managed-p2p")]
pub use defra_p2p_adapter::{ReplicatorPushOptions, ReplicatorPushOptionsState};
pub use dense_search::{DenseHybridSearchHit, DenseHybridSearchRequest, DenseHybridSearchResponse};
pub use error::{Error, Result};
pub use events::EventName;
pub use keyring;
pub use keyring::Keyring;
pub use lens::{LensConfig, LensModule, TransformId};
#[cfg(feature = "managed-p2p")]
pub use managed_p2p::{ManagedP2PSystem, ShutdownHandle, TransportKind};
pub use node_builder::{NodeBuilder, StorageBackend, StoreProfile};
pub use peer_keyring::PeerKeyring;
pub use query::QueryLimits;
pub use query::{QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};
pub use schema::CollectionVersion;
pub use telemetry::{ConflictMetricsSnapshot, RetryLayerSnapshot};
#[cfg(feature = "otel")]
pub use telemetry::{TelemetryConfig, TelemetryHandle};
pub use transaction::NodeTransaction;
pub use zeroize::Zeroizing;

use network::NodeEdge;
#[cfg(not(target_arch = "wasm32"))]
use signed_query::{execute_with_signing_context, unavailable_node_signer_response};

/// Type-erased schema operations so we can store DB<S> without leaking the Store generic.
#[async_trait::async_trait]
trait SchemaOps: Send + Sync {
    async fn add_collection(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> Result<Vec<CollectionVersion>>;
    async fn ensure_ordinary_index(
        &self,
        context: &RequestContext,
        collection: &str,
        name: &str,
        fields: Vec<schema::IndexedFieldDescription>,
    ) -> Result<schema::IndexDescription>;
    async fn add_view(
        &self,
        context: &RequestContext,
        source_query: &str,
        target_sdl: &str,
    ) -> Result<()>;
    async fn patch_collection(
        &self,
        context: &RequestContext,
        collection_name: &str,
        patch: &str,
    ) -> Result<CollectionVersion>;
    async fn set_active_collection_version(
        &self,
        context: &RequestContext,
        version_id: &str,
    ) -> Result<()>;
    async fn set_migration(
        &self,
        context: &RequestContext,
        config: LensConfig,
    ) -> Result<TransformId>;
    async fn materialize_collection(
        &self,
        context: &RequestContext,
        collection_name: &str,
    ) -> Result<usize>;
    fn list_collections(&self) -> Result<Vec<String>>;
    fn get_collection(&self, name: &str) -> Result<Option<CollectionVersion>>;
    async fn get_collection_by_version_id(
        &self,
        version_id: &str,
    ) -> Result<Option<CollectionVersion>>;
    async fn get_all_collection_versions(&self) -> Result<Vec<CollectionVersion>>;
}

#[cfg(feature = "http")]
struct HttpSchemaOps {
    schema_ops: Arc<dyn SchemaOps>,
    /// The request these operations serve, for ones bound to it.
    context: Option<RequestContext>,
}

#[cfg(feature = "http")]
#[async_trait::async_trait]
impl defra_http::router::CollectionOperations for HttpSchemaOps {
    fn bind_request(
        &self,
        context: &RequestContext,
    ) -> Option<Arc<dyn defra_http::router::CollectionOperations>> {
        Some(Arc::new(Self {
            schema_ops: Arc::clone(&self.schema_ops),
            context: Some(context.clone()),
        }))
    }

    async fn add_collection(&self, sdl: &str) -> Result<Vec<CollectionVersion>, String> {
        self.schema_ops
            .add_collection(
                &RequestContext::bound_or_ambient(self.context.as_ref()),
                sdl,
            )
            .await
            .map_err(|error| error.to_string())
    }
}

#[async_trait::async_trait]
trait BlockOps: Send + Sync {
    async fn signed_block_bytes(
        &self,
        cid: &str,
        caller_did: Option<&str>,
    ) -> Result<(Vec<u8>, Vec<u8>)>;
    async fn verified_signer_did(&self, cid: &str) -> Result<String>;
    async fn verified_signer_did_in_txn(
        &self,
        context: &RequestContext,
        cid: &str,
        transaction: &TransactionHandle,
    ) -> Result<String>;
}

/// An embedded DefraDB node with query execution and event subscription.
pub struct Node {
    database: Arc<db::DB<storage::RegolithStore>>,
    txn_registry: Arc<db::DbTransactionRegistry<storage::RegolithStore>>,
    nac_manager: Arc<dyn db::NacManagerApi>,
    local_zanzibar_store: Option<Arc<dyn acp::ZanzibarStore>>,
    #[cfg(feature = "vera")]
    vera_acp: Option<Arc<vera::VeraDocumentACP>>,
    query_limits: QueryLimits,
    runner: Arc<dyn QueryExecutor>,
    event_bus: Arc<dyn events::Bus>,
    schema_ops: Arc<dyn SchemaOps>,
    #[cfg(feature = "http")]
    collection_version_ops: Arc<dyn defra_http::router::CollectionVersionOperations>,
    block_ops: Arc<dyn BlockOps>,
    acp_ops: Arc<dyn acp_ops::AcpOps>,
    document_acp: Arc<dyn acp::DocumentACP>,
    embedding_config: db::EmbeddingClientConfig,
    node_identity_did: Option<String>,
    node_query_identity: Option<identity::Did>,
    /// The node's share of the network runtime; `None` when the node has
    /// no network work and was given no runtime.
    network: Option<NodeEdge>,
    /// The database's worker pool: the node's data-path calls run there.
    pool: Option<workers::Pool>,
    /// Document ACP is a remote provider: its calls reach the network, so
    /// they stay where they are made.
    remote_acp: bool,
    #[cfg(not(target_arch = "wasm32"))]
    transaction_stats: Option<storage::TransactionStatsHandle>,
    #[cfg(feature = "http")]
    txn_cleanup_task: kovan_queue::seg_queue::SegQueue<workers::Background>,
    /// Vector index rebuild, downsample view refresh, parked-block replay and
    /// the Vera redrive. Each holds the database open until aborted.
    #[cfg(not(target_arch = "wasm32"))]
    background_tasks: Arc<BackgroundTasks>,
    #[cfg(feature = "managed-p2p")]
    p2p: Option<Arc<ManagedP2PSystem>>,
    /// Idempotency guard for [`Node::shutdown`]: set by its first caller.
    shutdown_started: std::sync::atomic::AtomicBool,
    /// Set once the teardown has finished.
    shutdown_finished: std::sync::atomic::AtomicBool,
    /// Wakes concurrent shutdown callers once teardown is complete.
    shutdown_notify: regolith::sync::Notify,
    #[cfg(feature = "otel")]
    telemetry: kovan_queue::seg_queue::SegQueue<TelemetryHandle>,
}

#[cfg(feature = "http")]
#[derive(Clone, Copy)]
struct TransactionCleanupConfig {
    max_idle_age: Duration,
    sweep_interval: Duration,
}

#[cfg(any(feature = "http", feature = "otel"))]
fn single_slot<T>(value: Option<T>) -> kovan_queue::seg_queue::SegQueue<T>
where
    T: 'static,
{
    let slot = kovan_queue::seg_queue::SegQueue::new();
    if let Some(value) = value {
        slot.push(value);
    }
    slot
}

impl Node {
    /// Start building a new node.
    pub fn builder() -> NodeBuilder {
        NodeBuilder::default()
    }

    /// Execute a GraphQL query or mutation.
    ///
    /// A configured node identity is used as the document ACP actor. Prepared
    /// requests with an explicit identity retain that actor instead.
    pub async fn execute(&self, query_str: &str) -> QueryResponse {
        self.execute_request_once(QueryRequest::new(query_str))
            .await
    }

    /// Execute a prepared query request once, resolving external inputs between attempts.
    ///
    /// Storage conflicts are returned to the caller. Eligible implicit mutations
    /// retry inside the engine-owned transaction boundary.
    pub async fn execute_request(&self, request: QueryRequest) -> QueryResponse {
        self.execute_prepared_request(request, None).await
    }

    /// Execute a prepared query request within an existing transaction.
    ///
    /// The node identity context and explicit request identity are applied
    /// the same way as [`Self::execute_request`].
    pub async fn execute_request_in_txn(
        &self,
        request: QueryRequest,
        handle: &TransactionHandle,
    ) -> QueryResponse {
        self.execute_prepared_request(request, Some(handle.clone()))
            .await
    }

    async fn execute_request_once(&self, request: QueryRequest) -> QueryResponse {
        self.execute_prepared_request(request, None).await
    }

    async fn execute_prepared_request(
        &self,
        request: QueryRequest,
        txn_handle: Option<TransactionHandle>,
    ) -> QueryResponse {
        let request = self.with_default_query_identity(request);
        let Some(node_identity_did) = self.node_identity_did.as_deref() else {
            return match txn_handle {
                Some(handle) => self.runner.execute_in_txn(request, &handle).await,
                None => execute_autocommit_request(self.runner.as_ref(), request).await,
            };
        };

        #[cfg(target_arch = "wasm32")]
        return QueryResponse::error(format!(
            "configured node signing identity {node_identity_did} cannot execute queries on wasm32"
        ));

        #[cfg(not(target_arch = "wasm32"))]
        let signing_config = defra_core::signing::resolve_signing_config_with_flag(
            None,
            Some(node_identity_did),
            true,
        );
        #[cfg(not(target_arch = "wasm32"))]
        let Some(signing_config) = signing_config
        else {
            return unavailable_node_signer_response(node_identity_did);
        };
        #[cfg(not(target_arch = "wasm32"))]
        let Some(network) = self.network.as_ref() else {
            return unavailable_node_signer_response(node_identity_did);
        };

        #[cfg(not(target_arch = "wasm32"))]
        execute_with_signing_context(
            network.edge(),
            self.runner.clone(),
            request,
            txn_handle,
            signing_config,
            node_identity_did.to_string(),
        )
        .await
    }

    fn with_default_query_identity(&self, mut request: QueryRequest) -> QueryRequest {
        if request.identity.is_none() {
            request.identity.clone_from(&self.node_query_identity);
        }
        request
    }

    /// Runs the data-path call `make` builds on the database's worker pool
    /// (in place on a worker, or without a pool). The request context the
    /// call acts in travels inside it.
    async fn on_pool<F, Fut, T>(&self, make: F) -> Result<T>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        workers::run_on(self.pool.as_ref(), workers::Target::LeastLoaded, make)
            .await
            .map_err(Error::WorkerPoolCall)?
    }

    /// Runs a document ACP call: on the pool when the ACP is the node's own
    /// store, where it is made when the ACP is a remote provider.
    async fn on_acp<F, Fut, T>(&self, make: F) -> Result<T>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        if self.remote_acp {
            make().await
        } else {
            self.on_pool(make).await
        }
    }

    /// The context the node's own operations run in: the node's identity acts
    /// in it, so DB-layer NAC checks resolve to the node itself (which holds
    /// full access). Without a node identity it is [`RequestContext::ambient`].
    fn node_context(&self) -> RequestContext {
        let context = RequestContext::ambient();
        match &self.node_identity_did {
            Some(did) => context.with_identity(Some(did.clone())),
            None => context,
        }
    }

    /// DID used as the node identity for signing, when configured.
    pub fn node_identity_did(&self) -> Option<&str> {
        self.node_identity_did.as_deref()
    }

    /// Cryptographically verify a committed block and return its signer DID.
    ///
    /// This reads the signature linked by `cid`, verifies it over the
    /// canonical DAG-CBOR block bytes, and derives the DID from the verified
    /// public key. Unsigned, malformed, missing, or invalid blocks fail closed.
    pub async fn verified_block_signer_did(&self, cid: &str) -> Result<String> {
        let (ops, cid) = (Arc::clone(&self.block_ops), cid.to_owned());
        self.on_pool(move || async move { ops.verified_signer_did(&cid).await })
            .await
    }

    /// Load canonical signed-block material after applying document ACP for
    /// the supplied caller. The caller must independently verify both CIDs and
    /// the detached signature before trusting the returned signer.
    pub async fn authorized_signed_block_bytes(
        &self,
        cid: &str,
        caller_did: Option<&str>,
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        let (ops, cid) = (Arc::clone(&self.block_ops), cid.to_owned());
        let caller = caller_did.map(str::to_owned);
        self.on_pool(move || async move { ops.signed_block_bytes(&cid, caller.as_deref()).await })
            .await
    }

    /// Cryptographically verify a block visible inside an active transaction.
    ///
    /// This variant can verify uncommitted blocks before the caller commits or
    /// rolls back the transaction.
    pub async fn verified_block_signer_did_in_txn(
        &self,
        cid: &str,
        transaction: &TransactionHandle,
    ) -> Result<String> {
        self.block_ops
            .verified_signer_did_in_txn(&self.node_context(), cid, transaction)
            .await
    }

    /// Add a schema from a GraphQL SDL type definition.
    pub async fn add_collection(&self, sdl: &str) -> Result<()> {
        let (ops, sdl, context) = (
            Arc::clone(&self.schema_ops),
            sdl.to_owned(),
            self.node_context(),
        );
        self.on_pool(move || async move { ops.add_collection(&context, &sdl).await.map(|_| ()) })
            .await
    }

    /// Ensure a named nonunique ordinary index, preserving the collection's
    /// version and documents. Readiness is reported only after backfill; an
    /// existing unfinished or failed backfill returns an error.
    pub async fn ensure_ordinary_index(
        &self,
        collection: &str,
        name: &str,
        fields: Vec<schema::IndexedFieldDescription>,
    ) -> Result<schema::IndexDescription> {
        let (ops, context) = (Arc::clone(&self.schema_ops), self.node_context());
        let (collection, name) = (collection.to_owned(), name.to_owned());
        self.on_pool(move || async move {
            ops.ensure_ordinary_index(&context, &collection, &name, fields)
                .await
        })
        .await
    }

    /// Create a materialized view from a source query and target SDL.
    ///
    /// `source_query` format: `"SourceType { field1 field2 ... }"`
    /// `target_sdl` is the SDL for the view collection (may include directives
    /// like `@downsample` that are forward-declared for future defradb.rs support).
    pub async fn add_view(&self, source_query: &str, target_sdl: &str) -> Result<()> {
        let (ops, context) = (Arc::clone(&self.schema_ops), self.node_context());
        let (query, sdl) = (source_query.to_owned(), target_sdl.to_owned());
        self.on_pool(move || async move { ops.add_view(&context, &query, &sdl).await })
            .await
    }

    /// Apply a JSON Patch (RFC 6902) to an existing collection's schema.
    ///
    /// Returns the updated [`CollectionVersion`] (with a new `version_id`). The
    /// prior version is deactivated and the patched version is activated, unless
    /// the patch is a metadata-only or in-place change (see [`db::DB::patch_collection`]).
    ///
    /// `collection_name` may be a collection name, version ID, or variant; the
    /// underlying implementation falls back to version-ID lookup if name lookup fails.
    pub async fn patch_collection(
        &self,
        collection_name: &str,
        patch: &str,
    ) -> Result<CollectionVersion> {
        let (ops, context) = (Arc::clone(&self.schema_ops), self.node_context());
        let (name, patch) = (collection_name.to_owned(), patch.to_owned());
        self.on_pool(move || async move { ops.patch_collection(&context, &name, &patch).await })
            .await
    }

    /// Activate a specific collection version by its `version_id`.
    ///
    /// Deactivates sibling versions of the same collection and updates the
    /// collection-name pointer to resolve to this version. If migrations are
    /// registered, documents are reindexed through them.
    pub async fn set_active_collection_version(&self, version_id: &str) -> Result<()> {
        let (ops, context) = (Arc::clone(&self.schema_ops), self.node_context());
        let version = version_id.to_owned();
        self.on_pool(
            move || async move { ops.set_active_collection_version(&context, &version).await },
        )
        .await
    }

    /// Register a Lens migration between two collection versions.
    ///
    /// Returns the content-addressed [`TransformId`] of the stored transform.
    /// Placeholder versions are created if the source or destination are not
    /// yet materialized, allowing migrations to be registered ahead of patches.
    ///
    /// Requires the `wasmtime-runtime` feature (on by default). Without it this
    /// returns an explicit error instead of registering a no-op transform.
    pub async fn set_migration(&self, config: LensConfig) -> Result<TransformId> {
        let (ops, context) = (Arc::clone(&self.schema_ops), self.node_context());
        self.on_pool(move || async move { ops.set_migration(&context, config).await })
            .await
    }

    /// Eagerly migrate and cache every known-version document in a collection.
    ///
    /// Returns the number of documents advanced to the active version. This is a
    /// datastore-only operation and does not create or broadcast document commits.
    pub async fn materialize_collection(&self, collection_name: &str) -> Result<usize> {
        let (ops, context) = (Arc::clone(&self.schema_ops), self.node_context());
        let name = collection_name.to_owned();
        self.on_pool(move || async move { ops.materialize_collection(&context, &name).await })
            .await
    }

    /// List the names of every active collection known to the node.
    ///
    /// Useful for idempotent schema-bootstrap flows that need to decide whether
    /// to call [`Self::add_collection`] (create) or [`Self::patch_collection`] (evolve).
    pub fn list_collections(&self) -> Result<Vec<String>> {
        self.schema_ops.list_collections()
    }

    /// Fetch the active schema definition for a collection by name.
    ///
    /// Returns `Ok(None)` if no active collection with that name exists.
    pub fn get_collection(&self, name: &str) -> Result<Option<CollectionVersion>> {
        self.schema_ops.get_collection(name)
    }

    /// Fetch a collection schema by its version ID, including inactive versions.
    ///
    /// Searches both the in-memory cache (active versions) and the underlying
    /// systemstore (all stored versions), so callers can inspect the history
    /// of a patched collection.
    pub async fn get_collection_by_version_id(
        &self,
        version_id: &str,
    ) -> Result<Option<CollectionVersion>> {
        let (ops, version) = (Arc::clone(&self.schema_ops), version_id.to_owned());
        self.on_pool(move || async move { ops.get_collection_by_version_id(&version).await })
            .await
    }

    /// Return every collection version known to the node, active and inactive.
    pub async fn get_all_collection_versions(&self) -> Result<Vec<CollectionVersion>> {
        let ops = Arc::clone(&self.schema_ops);
        self.on_pool(move || async move { ops.get_all_collection_versions().await })
            .await
    }

    /// Subscribe to DefraDB events.
    pub fn subscribe(&self, event_names: &[EventName]) -> events::Subscription {
        self.event_bus.subscribe(event_names)
    }

    /// Subscribe to bounded current-state invalidations without retaining every
    /// historical revision. Subscribe before the initial query to avoid gaps.
    pub fn subscribe_document_changes(&self) -> events::DocumentChangeSubscription {
        self.event_bus.subscribe_document_changes()
    }

    /// [`Self::subscribe_document_changes`] limited to the given collections,
    /// identified by [`CollectionVersion::collection_id`], which is stable
    /// across schema versions. Updates elsewhere neither wake the subscription
    /// nor push it into `resync_required`.
    pub fn subscribe_collection_changes(
        &self,
        collection_ids: impl IntoIterator<Item = String>,
    ) -> events::DocumentChangeSubscription {
        self.event_bus
            .subscribe_collection_changes(collection_ids.into_iter().collect())
    }

    /// Begin a manually finalized transaction. Dropping its handle does not
    /// roll it back; use [`Self::begin_transaction_guard`] for scope ownership.
    pub async fn begin_transaction(
        &self,
        readonly: bool,
    ) -> Result<TransactionHandle, query::TransactionError> {
        self.runner
            .begin_txn_as(&self.node_context(), readonly)
            .await
    }

    /// Commit a transaction owned by this node.
    pub async fn commit_transaction(
        &self,
        transaction: &TransactionHandle,
    ) -> Result<(), query::TransactionError> {
        self.runner
            .commit_txn_as(&self.node_context(), transaction)
            .await
    }

    /// Roll back a transaction owned by this node.
    pub async fn discard_transaction(
        &self,
        transaction: &TransactionHandle,
    ) -> Result<(), query::TransactionError> {
        self.runner
            .discard_txn_as(&self.node_context(), transaction)
            .await
    }

    /// Access the raw query executor for advanced use.
    ///
    /// Calling `execute` or `execute_in_txn` on this executor bypasses the
    /// node's signing context and default ACP identity. Use [`Self::execute`]
    /// or [`Self::execute_request_in_txn`] for document reads and writes.
    pub fn runner(&self) -> &Arc<dyn QueryExecutor> {
        &self.runner
    }

    /// Access the event bus directly.
    pub fn event_bus(&self) -> &Arc<dyn events::Bus> {
        &self.event_bus
    }

    /// Register a DAC policy (Go: client.Store::AddDACPolicy). Returns the policy ID.
    pub async fn add_dac_policy(&self, identity: &str, policy: &str) -> Result<String> {
        let ops = Arc::clone(&self.acp_ops);
        let (identity, policy) = (identity.to_owned(), policy.to_owned());
        self.on_acp(move || async move { ops.add_dac_policy(&identity, &policy).await })
            .await
    }

    /// Register a DAC policy under an explicit id-salt counter (local ACP only).
    ///
    /// Local ACP policy ids are `sha256(policy fields || counter)` with a
    /// per-process counter, so two nodes that register the same policy after
    /// different histories derive different ids. A collection bound to a
    /// policy embeds the id in its SDL, and the SDL text determines the
    /// collection id — so every peer joining a policy-bound collection must
    /// reproduce the creator's exact policy id. Passing the creator's counter
    /// (typically 1, its first policy) makes the id reproducible, and storing
    /// an existing id overwrites it, so re-registration is idempotent.
    pub async fn add_dac_policy_with_counter(
        &self,
        identity: &str,
        policy: &str,
        counter: u64,
    ) -> Result<String> {
        let ops = Arc::clone(&self.acp_ops);
        let (identity, policy) = (identity.to_owned(), policy.to_owned());
        self.on_acp(move || async move {
            ops.add_dac_policy_with_counter(&identity, &policy, counter)
                .await
        })
        .await
    }

    /// Grant `target` the `relation` on a document (Go: client.Store::AddDACActorRelationship).
    ///
    /// `identity` is the requesting actor's DID; it must own the document or
    /// hold a relation that manages `relation`. `target` is an actor DID, the
    /// all-actors wildcard `*`, or a structured subject.
    ///
    /// Returns `existed_already`: `true` means the relationship was already
    /// present and the call was a no-op, `false` means it was newly added. A new
    /// grant also publishes a document-update event.
    pub async fn add_dac_actor_relationship(
        &self,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        let ops = Arc::clone(&self.acp_ops);
        let owned = |value: &str| value.to_owned();
        let (identity, collection, doc_id) = (owned(identity), owned(collection), owned(doc_id));
        let (relation, target) = (owned(relation), owned(target));
        self.on_acp(move || async move {
            ops.add_dac_actor_relationship(&identity, &collection, &doc_id, &relation, &target)
                .await
        })
        .await
    }

    /// Revoke `target`'s `relation` on a document (Go: client.Store::DeleteDACActorRelationship).
    ///
    /// `identity` is the requesting actor's DID; it must own the document or
    /// hold a relation that manages `relation`.
    ///
    /// Returns `record_found`: `true` means the relationship existed and was
    /// deleted, `false` means there was nothing to delete.
    pub async fn delete_dac_actor_relationship(
        &self,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        let ops = Arc::clone(&self.acp_ops);
        let owned = |value: &str| value.to_owned();
        let (identity, collection, doc_id) = (owned(identity), owned(collection), owned(doc_id));
        let (relation, target) = (owned(relation), owned(target));
        self.on_acp(move || async move {
            ops.delete_dac_actor_relationship(&identity, &collection, &doc_id, &relation, &target)
                .await
        })
        .await
    }

    /// Access the raw document ACP handle (Go: `node.DB.DocumentACP()`).
    ///
    /// This is an escape hatch for policy and relationship operations the node
    /// does not wrap. It bypasses node access control and the collection-policy
    /// lookup, so standard flows belong on [`Self::add_dac_policy`],
    /// [`Self::add_dac_actor_relationship`], and
    /// [`Self::delete_dac_actor_relationship`].
    pub fn document_acp(&self) -> Arc<dyn acp::DocumentACP> {
        self.document_acp.clone()
    }

    /// Access the resolved node-level embedding runtime config.
    pub fn embedding_config(&self) -> &db::EmbeddingClientConfig {
        &self.embedding_config
    }

    /// Capture backend-neutral transaction conflict and commit-gate diagnostics.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn transaction_stats(&self) -> Option<storage::TransactionStatsSnapshot> {
        self.transaction_stats
            .as_ref()
            .map(storage::TransactionStatsHandle::snapshot)
    }

    /// Capture process-lifetime transaction retry and client escape counters.
    pub fn conflict_metrics(&self) -> ConflictMetricsSnapshot {
        telemetry::conflict_metrics_snapshot()
    }

    /// Access P2P operations (if P2P is enabled and configured).
    #[cfg(feature = "managed-p2p")]
    pub fn p2p(&self) -> Option<&dyn defra_http::P2POperations> {
        self.p2p.as_deref().map(|system| system.ops().as_ref())
    }

    /// Cloneable P2P operations handle for background tasks.
    #[cfg(feature = "managed-p2p")]
    pub fn p2p_arc(&self) -> Option<Arc<dyn defra_http::P2POperations>> {
        self.p2p.as_ref().map(|system| Arc::clone(system.ops()))
    }

    /// The managed P2P system: transport kind, replicator push options, the
    /// retry trigger and the management requester.
    #[cfg(feature = "managed-p2p")]
    pub fn p2p_system(&self) -> Option<&Arc<ManagedP2PSystem>> {
        self.p2p.as_ref()
    }

    /// The database the node runs on.
    pub fn database(&self) -> &Arc<db::DB<storage::RegolithStore>> {
        &self.database
    }

    /// The registry of the node's explicit transactions.
    pub fn txn_registry(&self) -> &Arc<db::DbTransactionRegistry<storage::RegolithStore>> {
        &self.txn_registry
    }

    /// The node access control manager: off until enabled.
    pub fn nac_manager(&self) -> &Arc<dyn db::NacManagerApi> {
        &self.nac_manager
    }

    /// The local document-ACP policy store; `None` under Vera ACP.
    pub fn local_zanzibar_store(&self) -> Option<&Arc<dyn acp::ZanzibarStore>> {
        self.local_zanzibar_store.as_ref()
    }

    /// The Vera document ACP; `None` under local ACP.
    #[cfg(feature = "vera")]
    pub fn vera_acp(&self) -> Option<&Arc<vera::VeraDocumentACP>> {
        self.vera_acp.as_ref()
    }

    /// The limits queries on this node run under.
    pub fn query_limits(&self) -> QueryLimits {
        self.query_limits
    }

    /// The tasks the node owns; stopping them frees the database to close.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn background_tasks(&self) -> Arc<BackgroundTasks> {
        Arc::clone(&self.background_tasks)
    }

    /// Returns true once [`Node::shutdown`] has been initiated; it
    /// does not say the teardown has finished.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown_started
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// [`Self::execute`] as `context` serves, rather than as the node.
    pub async fn execute_as(&self, context: &RequestContext, query_str: &str) -> QueryResponse {
        self.runner
            .execute(QueryRequest::new(query_str).with_context(context.clone()))
            .await
    }

    /// Add an equality encrypted (searchable-encryption) index on `field_name`
    /// of `collection`, so the node can write SE artifacts and act as an SE
    /// query owner (#976).
    pub async fn add_encrypted_index(&self, collection: &str, field_name: &str) -> Result<()> {
        let database = Arc::clone(&self.database);
        let (collection, field_name) = (collection.to_owned(), field_name.to_owned());
        self.on_pool(move || add_encrypted_index(database, collection, field_name))
            .await
    }

    /// Authorize a peer to open an inbound P2P connection to this node while
    /// it is running, without a restart.
    ///
    /// Widens who may connect in: it does not itself dial, connect to, or
    /// disconnect from the peer, and it is a no-op when the active transport
    /// already accepts every inbound peer. Returns an error if P2P is not
    /// enabled, or if the active transport has no concept of an inbound
    /// allowlist.
    #[cfg(feature = "managed-p2p")]
    pub async fn allow_p2p_peer(&self, peer_id: &str) -> Result<()> {
        let ops = self.p2p().ok_or(Error::P2pDisabled)?;
        let peer_id = defra_http::TransportPeerId::new(peer_id).map_err(Error::InvalidPeerId)?;
        ops.allow_peer(&peer_id).await.map_err(Error::PeerAuthorize)
    }

    /// Bar a peer from this node in both directions, while it is running and
    /// without a restart. This is the shape a device logout takes.
    ///
    /// Stronger than the inverse of [`Self::allow_p2p_peer`]. It refuses the
    /// peer's next inbound connection, refuses this node's own outbound dials
    /// to it, and closes every connection it currently holds, so a revoked
    /// peer can neither keep a session it already opened nor be reached for
    /// again by this node's own reconnect logic. The bar is recorded before
    /// anything is closed, so a reconnect racing this call cannot be
    /// re-admitted in between.
    ///
    /// Destructive, and not undone by `allow_p2p_peer`: the peer's replicator
    /// registration and its durable retry record are deleted, because
    /// otherwise this node would keep dialling a peer it has barred.
    /// Re-admitting the device restores its ability to connect, not its
    /// replication, which has to be added back explicitly.
    ///
    /// This does not guarantee that a request already being served over a
    /// closed connection is aborted mid-exchange.
    ///
    /// Returns an error if P2P is not enabled, or if the active transport has
    /// no concept of peer admission.
    #[cfg(feature = "managed-p2p")]
    pub async fn deny_p2p_peer(&self, peer_id: &str) -> Result<()> {
        let ops = self.p2p().ok_or(Error::P2pDisabled)?;
        let peer_id = defra_http::TransportPeerId::new(peer_id).map_err(Error::InvalidPeerId)?;
        ops.deny_peer(&peer_id).await.map_err(Error::PeerRevoke)
    }

    /// Gracefully stop background services owned by this node.
    ///
    /// **The node should not be used after this call.** When the `otel`
    /// feature is on, `shutdown` calls `TelemetryHandle::shutdown` which
    /// flushes and disables the global tracer provider. Subsequent calls
    /// that emit spans (e.g. `execute`, `add_collection`) go to a no-op
    /// tracer — they still work functionally, but observability is gone
    /// with no error surfaced. Drop the node after shutdown completes.
    pub async fn shutdown(&self) {
        // The first caller tears the node down; a concurrent caller waits for
        // that teardown, so `shutdown().await` always means it has finished.
        if self
            .shutdown_started
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            self.wait_for_shutdown().await;
            return;
        }
        tracing::info!("node shutdown: begin");

        #[cfg(feature = "http")]
        if let Some(task) = self.txn_cleanup_task.pop() {
            task.abort();
            let _ = workers::timeout(Duration::from_secs(1), task).await;
        }

        #[cfg(not(target_arch = "wasm32"))]
        self.background_tasks.shutdown().await;

        // The transport and the tasks around it stop first and are awaited, so
        // none still reaches storage when the drain and the close come.
        #[cfg(feature = "managed-p2p")]
        if let Some(p2p) = &self.p2p {
            p2p.stop_transport().await;
        }

        // Stop accepting the node's network work (signed queries, the HTTP
        // server's accept loop) and drain what is in flight.
        if let Some(network) = &self.network {
            network.drain().await;
        }

        // Nothing reaches the data path any more: the worker pool finishes
        // what it admitted and stops, then the store closes.
        if let Err(error) = self.database.close().await {
            tracing::warn!(%error, "the node's database did not close cleanly");
        }

        // Flush buffered spans / metrics. Done after P2P shutdown so trailing
        // shutdown spans are still captured. Go DefraDB never calls provider
        // shutdown — we don't repeat that bug.
        //
        // `handle.shutdown()` synchronously joins the SDK batch-exporter
        // thread (up to ~5 s, double with metrics), so run it on a blocking
        // thread of the network runtime rather than stalling the caller.
        #[cfg(feature = "otel")]
        if let Some(handle) = self.telemetry.pop() {
            let runtime = self
                .network
                .as_ref()
                .map(|network| Ok(network.runtime().clone()))
                .unwrap_or_else(|| edge::on_demand().cloned());
            // The slot keeps the handle when the runtime refuses the work,
            // so its flush moves to a thread of its own instead of running
            // on the caller as the dropped closure's handle.
            let slot = Arc::new(single_slot(Some(handle)));
            let flushing = Arc::clone(&slot);
            let flushed = match runtime {
                Ok(runtime) => runtime
                    .spawn_blocking(move || {
                        if let Some(handle) = flushing.pop() {
                            handle.shutdown();
                        }
                    })
                    .await
                    .map_err(|error| error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            if let Err(error) = flushed {
                tracing::warn!(%error, "telemetry did not flush on the network runtime");
            }
            if let Some(handle) = slot.pop() {
                if let Err(error) = std::thread::Builder::new()
                    .name("defra-otel-shutdown".to_string())
                    .spawn(move || handle.shutdown())
                {
                    tracing::warn!(%error, "telemetry could not start its flush thread");
                }
            }
        }

        // Last: a runtime the node started itself goes once nothing of the
        // node runs on it.
        if let Some(network) = &self.network {
            network.stop_owned().await;
        }

        self.shutdown_finished
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.shutdown_notify.notify_waiters();
        tracing::info!("node shutdown: complete");
    }

    async fn wait_for_shutdown(&self) {
        loop {
            let notified = self.shutdown_notify.notified();
            if self
                .shutdown_finished
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                return;
            }
            notified.await;
        }
    }
}

/// Adds the encrypted index [`Node::add_encrypted_index`] names, on
/// the worker it runs on.
async fn add_encrypted_index(
    database: Arc<db::DB<storage::RegolithStore>>,
    collection: String,
    field_name: String,
) -> Result<()> {
    use storage::corekv::Key;

    let (collection, field_name) = (collection.as_str(), field_name.as_str());
    let col = database
        .get_collection(collection)
        .map_err(Error::CollectionLookupFailed)?
        .ok_or_else(|| Error::CollectionMissing(collection.to_owned()))?;
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

async fn execute_autocommit_request(
    executor: &dyn QueryExecutor,
    request: QueryRequest,
) -> QueryResponse {
    // Remote preparation is resolved outside mutation attempts. Storage and
    // business conflicts are returned after one attempt.
    let prepared = match executor.prepare_request(&request).await {
        Ok(prepared) => prepared,
        Err(error) => {
            return QueryResponse {
                data: None,
                errors: vec![query::QueryResponseError::from_query_error(error)],
                extensions: None,
            }
        }
    };
    execute_prepared_request_resolving_inputs(executor, request, prepared).await
}

async fn execute_prepared_request_resolving_inputs(
    executor: &dyn QueryExecutor,
    request: QueryRequest,
    mut prepared: Option<std::sync::Arc<query::prepared::PreparedMutations>>,
) -> QueryResponse {
    loop {
        let response = executor
            .execute_prepared(request.clone(), prepared.clone())
            .await;
        if let Some(input) = response.retry_input() {
            match executor
                .resolve_retry_input(&request, prepared.clone(), input)
                .await
            {
                Ok(Some(updated)) => {
                    prepared = Some(updated);
                    continue;
                }
                Ok(None) => {
                    return QueryResponse::error(
                        "retry input resolution returned no preparation capsule",
                    )
                }
                Err(error) => {
                    return QueryResponse::error(query::QueryResponseError::from_query_error(error))
                }
            }
        }
        return response;
    }
}

#[cfg(test)]
#[path = "request_preparation_tests.rs"]
mod request_preparation_tests;

fn timeout_secs(timeout: Duration) -> u64 {
    if timeout.is_zero() {
        0
    } else {
        timeout
            .as_secs()
            .saturating_add(u64::from(timeout.subsec_nanos() > 0))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use defra_core::signing::{RemoteSigner, SigningConfig, SigningKeyType};
    use regolith::sync::Mutex;

    use super::Node;

    #[cfg(feature = "http")]
    use axum::{routing::get, Router};

    #[cfg(feature = "http")]
    use super::HttpConfig;

    pub(super) static SIGNING_STORE_GUARD: Mutex<()> = Mutex::new(());
    #[cfg(feature = "http")]
    #[test]
    fn http_config_accepts_extra_routes() {
        use std::time::Duration;

        let config = HttpConfig::new(9182)
            .with_request_timeout(Duration::from_secs(120))
            .with_transaction_idle_timeout(Duration::from_secs(900))
            .with_transaction_cleanup_interval(Duration::from_secs(30))
            .with_extra_routes(Router::new().route("/healthz", get(|| async { "ok" })));

        assert_eq!(config.address.port(), 9182);
        assert_eq!(config.request_timeout, Duration::from_secs(120));
        assert_eq!(config.transaction_idle_timeout, Duration::from_secs(900));
        assert_eq!(config.transaction_cleanup_interval, Duration::from_secs(30));
        assert!(config.extra_routes.is_some());
    }

    #[cfg(feature = "http")]
    #[test]
    fn embedded_http_exposes_schema_add_and_only_read_collection_versions() {
        edge::testing::edge()
            .block_on(async {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let address = listener.local_addr().unwrap();

                let node = Node::builder()
                    .with_http(HttpConfig::with_listener(listener).unwrap())
                    .build()
                    .await
                    .unwrap();
                let client = reqwest::Client::new();
                let schema_url = format!("http://{address}/api/v0/collections");
                let created = edge::testing::edge()
                    .timeout(std::time::Duration::from_secs(5), async {
                        loop {
                            if let Ok(response) = client
                                .post(&schema_url)
                                .header(reqwest::header::CONTENT_TYPE, "text/plain")
                                .body("type Book { title: String }")
                                .send()
                                .await
                            {
                                if response.status().is_success() {
                                    break response
                                        .json::<Vec<schema::CollectionVersion>>()
                                        .await
                                        .unwrap();
                                }
                            }
                            edge::testing::edge()
                                .sleep(std::time::Duration::from_millis(20))
                                .await;
                        }
                    })
                    .await
                    .expect("embedded HTTP server did not expose schema add");
                assert_eq!(created.len(), 1);
                assert_eq!(created[0].name, "Book");

                let schema = client
                    .get(format!("http://{address}/api/v0/schema"))
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap()
                    .text()
                    .await
                    .unwrap();
                assert!(schema.contains("type Book"));

                let url = format!("http://{address}/api/v0/collections?get_inactive=true");
                let versions = edge::testing::edge()
                    .timeout(std::time::Duration::from_secs(5), async {
                        loop {
                            if let Ok(response) = client.get(&url).send().await {
                                if response.status().is_success() {
                                    break response
                                        .json::<Vec<schema::CollectionVersion>>()
                                        .await
                                        .unwrap();
                                }
                            }
                            edge::testing::edge()
                                .sleep(std::time::Duration::from_millis(20))
                                .await;
                        }
                    })
                    .await
                    .expect("embedded HTTP server did not expose collection versions");

                let book = versions
                    .iter()
                    .find(|version| version.name == "Book")
                    .expect("Book collection version should be returned");
                assert!(!book.collection_id.is_empty());
                assert!(!book.version_id.is_empty());

                let delete_response = client
                    .delete(format!("http://{address}/api/v0/collections/versions"))
                    .json(&vec![book.version_id.clone()])
                    .send()
                    .await
                    .unwrap();
                assert_eq!(
                    delete_response.status(),
                    reqwest::StatusCode::SERVICE_UNAVAILABLE,
                    "embedded collection observation must not enable destructive management"
                );

                node.shutdown().await;
            })
            .unwrap()
    }

    #[test]
    fn timeout_secs_rounds_non_zero_duration_up() {
        assert_eq!(super::timeout_secs(std::time::Duration::ZERO), 0);
        assert_eq!(super::timeout_secs(std::time::Duration::from_millis(1)), 1);
        assert_eq!(super::timeout_secs(std::time::Duration::new(2, 1)), 3);
        assert_eq!(super::timeout_secs(std::time::Duration::from_secs(5)), 5);
    }

    #[test]
    fn node_identity_did_requires_registered_signer() {
        edge::testing::edge()
            .block_on(async {
                let _serial = SIGNING_STORE_GUARD.lock().await;
                defra_core::signing::clear_identity_store();

                let error = match Node::builder()
                    .with_node_identity_did("did:key:zMissing")
                    .build()
                    .await
                {
                    Ok(_) => panic!("unregistered node identity must fail"),
                    Err(error) => error,
                };

                assert!(
                    error
                        .to_string()
                        .contains("is not registered in the DefraDB signing registry"),
                    "{error:#}"
                );
            })
            .unwrap()
    }

    #[test]
    fn node_identity_did_accepts_registered_remote_signer() {
        edge::testing::edge()
            .block_on(async {
                let _serial = SIGNING_STORE_GUARD.lock().await;
                defra_core::signing::clear_identity_store();

                let did = "did:key:zRegisteredRemote";
                defra_core::signing::store_identity(
                    did,
                    SigningConfig {
                        key_type: SigningKeyType::Secp256r1,
                        private_key_bytes: Vec::new(),
                        public_key_bytes: vec![2, 3, 4],
                        public_key_hex: "020304".to_string(),
                        remote_signer: Some(Arc::new(TestRemoteSigner)),
                        signing_authorization: None,
                    },
                );

                let node = Node::builder()
                    .with_node_identity_did(did)
                    .build()
                    .await
                    .expect("registered remote signer should build");

                assert_eq!(node.node_identity_did(), Some(did));
                node.shutdown().await;
                defra_core::signing::clear_identity_store();
            })
            .unwrap()
    }

    pub(super) struct TestRemoteSigner;

    impl RemoteSigner for TestRemoteSigner {
        fn sign_sync(
            &self,
            _data: &[u8],
            _authorization: Option<&defra_core::signing::SigningAuthorization>,
        ) -> Result<Vec<u8>, String> {
            Ok(vec![1, 2, 3])
        }
    }
}

#[cfg(all(test, feature = "p2p"))]
mod p2p_bench;

#[cfg(all(test, feature = "p2p"))]
mod p2p_tests;

#[cfg(test)]
mod owner_observation_tests;

#[cfg(all(test, feature = "http"))]
mod http_signing_tests;

#[cfg(all(test, feature = "http"))]
mod router_transform_tests;

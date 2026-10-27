//! Reusable embedded DefraDB node builder.
//!
//! Wraps defradb.rs library crates behind a clean builder API so that
//! downstream binaries can embed a DefraDB instance without duplicating
//! wiring code.
//!
//! P2P uses Iroh/QUIC only. libp2p lives on CLI, FFI, and `embedded`.
//!
//! ## Cargo features
//!
//! - `native` — native host (event channel). Default-on.
//! - `vera` — on-chain document ACP. Default-on. Omit for local-only ACP.
//! - `wasmtime-runtime` — Lens WASM execution. Default-on. Without it,
//!   [`EmbeddedNode::set_migration`] returns an explicit error.
//! - `p2p` — Iroh/QUIC replication. Implies `native`. Does **not** compile libp2p.
//! - `http` — GraphQL HTTP server.
//! - `otel` — OpenTelemetry exporter.

mod acp_ops;
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
#[cfg(test)]
mod embedded_query_identity_tests;
mod error;
mod network;
mod node_acp;
#[cfg(test)]
mod on_pool_tests;
#[cfg(feature = "p2p")]
mod p2p_runtime;
mod signed_query;
mod transaction;
pub mod version;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use defra_core::signing::SigningConfig;
use defra_core::RequestContext;
use identity::{Identity as _, IdentityKeyType, RawIdentity};

/// Re-exported so callers can name `Arc<dyn DocumentACP>` (the type
/// [`EmbeddedNode::document_acp`] returns) without depending on `acp` directly.
pub use acp::{DocumentACP, DocumentPermission, Identity as AcpIdentity};
pub use coding_search::{
    CodingHybridSearchHit, CodingHybridSearchRequest, CodingHybridSearchResponse,
    CodingSearchTarget,
};
pub use config::DocumentAcpConfig;
#[cfg(feature = "http")]
pub use config::HttpConfig;
#[cfg(feature = "p2p")]
pub use config::P2PConfig;
#[cfg(feature = "vera")]
pub use config::VeraConfig;
pub use dense_search::{DenseHybridSearchHit, DenseHybridSearchRequest, DenseHybridSearchResponse};
pub use error::{Error, Result};
pub use events::EventName;
pub use lens::{LensConfig, LensModule, TransformId};
pub use query::QueryLimits;
pub use query::{QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};
pub use schema::CollectionVersion;
pub use telemetry::{ConflictMetricsSnapshot, RetryLayerSnapshot};
#[cfg(feature = "otel")]
pub use telemetry::{TelemetryConfig, TelemetryHandle};
pub use transaction::EmbeddedTransaction;

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
pub struct EmbeddedNode {
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
    /// Closes the database: its worker pool finishes and stops, then its
    /// store closes. Run once, at shutdown.
    close_database: CloseDatabase,
    /// The database's worker pool: the node's data-path calls run there.
    pool: Option<workers::Pool>,
    /// Document ACP is a remote provider: its calls reach the network, so
    /// they stay where they are made.
    remote_acp: bool,
    #[cfg(not(target_arch = "wasm32"))]
    transaction_stats: Option<storage::TransactionStatsHandle>,
    #[cfg(feature = "http")]
    txn_cleanup_task: kovan_queue::seg_queue::SegQueue<workers::Background>,
    /// Vector index rebuild and downsample view refresh, as the CLI and the
    /// `embedded` node run them. Each holds the database open until aborted.
    #[cfg(not(target_arch = "wasm32"))]
    background_tasks: BackgroundTasks,
    #[cfg(feature = "p2p")]
    p2p_ops: Option<Arc<dyn defra_http::P2POperations>>,
    #[cfg(feature = "p2p")]
    p2p_lifecycle: Option<p2p_runtime::P2PLifecycle>,
    #[cfg(feature = "otel")]
    telemetry: kovan_queue::seg_queue::SegQueue<TelemetryHandle>,
}

/// Closes a node's database, whatever store it runs on.
type CloseDatabase = Box<
    dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = db::Result<()>> + Send>>
        + Send
        + Sync,
>;

/// The closer of `database`, for a node that does not know its store type.
fn close_database<S>(database: &Arc<db::DB<S>>) -> CloseDatabase
where
    S: storage::corekv::Store + 'static,
{
    let database = Arc::clone(database);
    Box::new(move || {
        let database = Arc::clone(&database);
        Box::pin(async move { database.close().await })
    })
}

#[cfg(not(target_arch = "wasm32"))]
struct BackgroundTasks(kovan_queue::seg_queue::SegQueue<workers::Background>);

#[cfg(not(target_arch = "wasm32"))]
impl BackgroundTasks {
    fn new(tasks: impl IntoIterator<Item = workers::Background>) -> Self {
        let queue = kovan_queue::seg_queue::SegQueue::new();
        for task in tasks {
            queue.push(task);
        }
        Self(queue)
    }

    fn take(&self) -> impl Iterator<Item = workers::Background> + '_ {
        std::iter::from_fn(|| self.0.pop())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for BackgroundTasks {
    fn drop(&mut self) {
        for task in self.take() {
            task.abort();
        }
    }
}

#[cfg(feature = "http")]
#[derive(Clone, Copy)]
struct TransactionCleanupConfig {
    max_idle_age: Duration,
    sweep_interval: Duration,
}

#[cfg(any(feature = "http", feature = "otel"))]
fn single_slot<T: 'static>(value: Option<T>) -> kovan_queue::seg_queue::SegQueue<T> {
    let slot = kovan_queue::seg_queue::SegQueue::new();
    if let Some(value) = value {
        slot.push(value);
    }
    slot
}

impl EmbeddedNode {
    /// Start building a new embedded node.
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

    /// DID used as the embedded node identity for signing, when configured.
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

    /// Commit a transaction owned by this embedded node.
    pub async fn commit_transaction(
        &self,
        transaction: &TransactionHandle,
    ) -> Result<(), query::TransactionError> {
        self.runner
            .commit_txn_as(&self.node_context(), transaction)
            .await
    }

    /// Roll back a transaction owned by this embedded node.
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
    #[cfg(feature = "p2p")]
    pub fn p2p(&self) -> Option<&dyn defra_http::P2POperations> {
        self.p2p_ops.as_deref()
    }

    /// Cloneable P2P operations handle for background tasks.
    #[cfg(feature = "p2p")]
    pub fn p2p_arc(&self) -> Option<Arc<dyn defra_http::P2POperations>> {
        self.p2p_ops.as_ref().map(Arc::clone)
    }

    /// Authorize a peer to open an inbound P2P connection to this node while
    /// it is running, without a restart.
    ///
    /// Widens who may connect in: it does not itself dial, connect to, or
    /// disconnect from the peer, and it is a no-op when the active transport
    /// already accepts every inbound peer. Returns an error if P2P is not
    /// enabled, or if the active transport has no concept of an inbound
    /// allowlist.
    #[cfg(feature = "p2p")]
    pub async fn allow_p2p_peer(&self, peer_id: &str) -> Result<()> {
        let ops = self.p2p_ops.as_ref().ok_or(Error::P2pDisabled)?;
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
    #[cfg(feature = "p2p")]
    pub async fn deny_p2p_peer(&self, peer_id: &str) -> Result<()> {
        let ops = self.p2p_ops.as_ref().ok_or(Error::P2pDisabled)?;
        let peer_id = defra_http::TransportPeerId::new(peer_id).map_err(Error::InvalidPeerId)?;
        ops.deny_peer(&peer_id).await.map_err(Error::PeerRevoke)
    }

    /// Gracefully stop background services owned by this embedded node.
    ///
    /// **The node should not be used after this call.** When the `otel`
    /// feature is on, `shutdown` calls `TelemetryHandle::shutdown` which
    /// flushes and disables the global tracer provider. Subsequent calls
    /// that emit spans (e.g. `execute`, `add_collection`) go to a no-op
    /// tracer — they still work functionally, but observability is gone
    /// with no error surfaced. Drop the node after shutdown completes.
    pub async fn shutdown(&self) {
        #[cfg(feature = "http")]
        if let Some(task) = self.txn_cleanup_task.pop() {
            task.abort();
            let _ = workers::timeout(Duration::from_secs(1), task).await;
        }

        #[cfg(not(target_arch = "wasm32"))]
        for task in self.background_tasks.take() {
            task.abort();
            // An aborted task keeps the database open until it is next polled.
            let _ = workers::timeout(Duration::from_secs(1), task).await;
        }

        // Stop accepting the node's network work (signed queries, the HTTP
        // server's accept loop) and drain what is in flight.
        if let Some(network) = &self.network {
            network.drain().await;
        }

        #[cfg(feature = "p2p")]
        if let Some(lifecycle) = &self.p2p_lifecycle {
            lifecycle.shutdown().await;
        }

        // Nothing reaches the data path any more: the worker pool finishes
        // what it admitted and stops, then the store closes.
        if let Err(error) = (self.close_database)().await {
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
    }
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

fn resolve_registered_node_identity(did: &str) -> Result<SigningConfig> {
    let config = defra_core::signing::get_identity(did)
        .ok_or_else(|| Error::NodeIdentityUnregistered(did.to_owned()))?;
    if !config.has_local_private_key() && !config.has_remote_signer() {
        return Err(Error::NodeIdentityUnusable(did.to_owned()));
    }
    Ok(config)
}

fn local_raw_identity_from_registered_config(
    did: &str,
    config: &SigningConfig,
) -> Result<Option<RawIdentity>> {
    if !config.has_local_private_key() {
        return Ok(None);
    }

    let key_type = identity_key_type_from_signing_key_type(config.key_type)?;
    let identity = RawIdentity::from_identity_key_type(key_type, &config.private_key_bytes)
        .map_err(|error| Error::NodeIdentityLoad {
            did: did.to_owned(),
            source: error,
        })?;
    let derived_did = identity
        .did()
        .map_err(Error::NodeIdentityDerive)?
        .to_string();
    if derived_did != did {
        return Err(Error::NodeIdentityMismatch {
            expected: did.to_owned(),
            derived: derived_did,
        });
    }
    Ok(Some(identity))
}

fn identity_key_type_from_signing_key_type(
    key_type: defra_core::signing::SigningKeyType,
) -> Result<IdentityKeyType> {
    match key_type {
        defra_core::signing::SigningKeyType::Ed25519 => Ok(IdentityKeyType::Ed25519),
        defra_core::signing::SigningKeyType::Secp256k1 => Ok(IdentityKeyType::Secp256k1),
        defra_core::signing::SigningKeyType::Secp256r1 => Ok(IdentityKeyType::Secp256r1),
        unsupported => Err(Error::UnsupportedKeyType(unsupported.to_string())),
    }
}

fn timeout_secs(timeout: Duration) -> u64 {
    if timeout.is_zero() {
        0
    } else {
        timeout
            .as_secs()
            .saturating_add(u64::from(timeout.subsec_nanos() > 0))
    }
}

/// Selects which persistent storage backend to use when `data_path` is set.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub enum StorageBackend {
    /// regolith, the one store. Kept as an enum so `with_storage_backend`
    /// stays on the builder and a later variant does not churn it.
    #[default]
    Regolith,
}

/// Builder for constructing an `EmbeddedNode`.
#[derive(Default)]
pub struct NodeBuilder {
    edge: Option<edge::Edge>,
    workers: Option<workers::Config>,
    data_path: Option<PathBuf>,
    storage_backend: StorageBackend,
    storage_durability: storage::backends::DurabilityMode,
    regolith_options: Option<storage::RegolithStoreOptions>,
    embedding_url: Option<String>,
    embedding_model: Option<String>,
    embedding_api_key: Option<String>,
    document_acp: DocumentAcpConfig,
    node_identity_did: Option<String>,
    node_acp_enabled: bool,
    at_rest_encryption_key: Option<[u8; 32]>,
    previous_at_rest_encryption_keys: Vec<[u8; 32]>,
    max_txn_retries: Option<u32>,
    query_timeout: Option<Duration>,
    query_limits: QueryLimits,
    introspection_cache: Option<bool>,
    #[cfg(feature = "http")]
    http_config: Option<HttpConfig>,
    #[cfg(feature = "p2p")]
    p2p_config: Option<P2PConfig>,
    #[cfg(feature = "otel")]
    telemetry_handle: Option<TelemetryHandle>,
    crdts: Vec<crdt::Registration>,
}

/// Durability is a separate builder setting, so it is applied over the
/// caller's options rather than being one of the values they replace.
fn resolve_regolith_options(
    explicit: Option<storage::RegolithStoreOptions>,
    durability: storage::backends::DurabilityMode,
) -> storage::RegolithStoreOptions {
    explicit.unwrap_or_default().with_durability(durability)
}

/// The builder's at-rest keys, current and previous, installed on `options`.
fn with_at_rest_keys(
    options: storage::RegolithStoreOptions,
    current: Option<[u8; 32]>,
    previous: &[[u8; 32]],
) -> Result<storage::RegolithStoreOptions> {
    match current {
        Some(key) => options
            .with_encryption_keys(key, previous.iter().copied())
            .map_err(Error::InvalidAtRestKeys),
        None => Ok(options),
    }
}

struct StoreBuildArgs {
    network: Option<NodeEdge>,
    persistence: node_acp::Persistence,
    document_acp_config: DocumentAcpConfig,
    db_options: db::DbOptions,
    event_bus: Arc<dyn events::Bus>,
    node_identity_did: Option<String>,
    #[cfg(feature = "p2p")]
    node_p2p_identity: Option<Arc<RawIdentity>>,
    node_acp_enabled: bool,
    query_timeout: Option<Duration>,
    query_limits: QueryLimits,
    introspection_cache: bool,
    #[cfg(feature = "http")]
    transaction_cleanup_config: Option<TransactionCleanupConfig>,
    #[cfg(feature = "p2p")]
    p2p_config: Option<P2PConfig>,
    #[cfg(feature = "otel")]
    telemetry_handle: Option<TelemetryHandle>,
}

struct PersistentStoreBuildArgs {
    network: Option<NodeEdge>,
    document_acp_config: DocumentAcpConfig,
    db_options: db::DbOptions,
    event_bus: Arc<dyn events::Bus>,
    node_identity_did: Option<String>,
    #[cfg(feature = "p2p")]
    node_p2p_identity: Option<Arc<RawIdentity>>,
    node_acp_enabled: bool,
    query_timeout: Option<Duration>,
    query_limits: QueryLimits,
    introspection_cache: bool,
    #[cfg(feature = "http")]
    transaction_cleanup_config: Option<TransactionCleanupConfig>,
    #[cfg(feature = "p2p")]
    p2p_config: Option<P2PConfig>,
    #[cfg(feature = "otel")]
    telemetry_handle: Option<TelemetryHandle>,
}

impl NodeBuilder {
    /// The worker pool the node's data path runs on. Unset, the pool's own
    /// defaults (`workers::Config::default()`).
    pub fn with_workers(mut self, config: workers::Config) -> Self {
        self.workers = Some(config);
        self
    }

    /// Run the node's network work (signed queries, the HTTP server, P2P)
    /// on the application's network runtime. Without one, the node starts
    /// its own when it has network work to run, and stops it at shutdown.
    pub fn with_edge(mut self, edge: edge::Edge) -> Self {
        self.edge = Some(edge);
        self
    }

    /// Set the data directory for persistent storage.
    /// If not set, uses in-memory storage.
    pub fn data_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.data_path = Some(path.into());
        self
    }

    /// Select the persistent storage backend (default: `Redb`).
    ///
    /// Has no effect when `data_path` is not set (in-memory mode).
    /// Using a backend requires that backend feature to be enabled.
    pub fn with_storage_backend(mut self, backend: StorageBackend) -> Self {
        self.storage_backend = backend;
        self
    }

    /// Set the persistent storage durability mode (default: `Immediate`).
    ///
    /// `Immediate` fsyncs acknowledged commits. `Eventual` can improve write
    /// throughput, but OS crashes may lose acknowledged writes.
    pub fn with_storage_durability(
        mut self,
        durability: storage::backends::DurabilityMode,
    ) -> Self {
        self.storage_durability = durability;
        self
    }

    /// Set the regolith store options, including its memory budget.
    ///
    /// Defaults to [`storage::RegolithStoreOptions::default`], whose sizing
    /// targets a server. An embedded host that opens the node in-process
    /// pays that budget out of its own address space, so it picks a profile
    /// here rather than inheriting one. Durability stays with
    /// [`NodeBuilder::with_storage_durability`] and is applied over whatever
    /// is set here.
    pub fn with_regolith_options(mut self, options: storage::RegolithStoreOptions) -> Self {
        self.regolith_options = Some(options);
        self
    }

    /// Enable engine file encryption at rest for the persistent storage backend.
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

    /// Set the maximum number of auto-commit transaction conflict retries.
    pub fn with_max_txn_retries(mut self, retries: u32) -> Self {
        self.max_txn_retries = Some(retries);
        self
    }

    /// Set the fallback OpenAI-compatible embedding base URL.
    pub fn with_embedding_url(mut self, url: impl Into<String>) -> Self {
        self.embedding_url = Some(url.into());
        self
    }

    /// Set the fallback embedding model name used when the schema leaves it empty.
    pub fn with_embedding_model(mut self, model: impl Into<String>) -> Self {
        self.embedding_model = Some(model.into());
        self
    }

    /// Set the resolved embedding API key value used for Authorization headers.
    pub fn with_embedding_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.embedding_api_key = Some(api_key.into());
        self
    }

    /// Configure the node to use Vera-backed document ACP.
    ///
    /// Requires the `vera` feature (on by default).
    #[cfg(feature = "vera")]
    pub fn with_vera(mut self, config: VeraConfig) -> Self {
        self.document_acp = DocumentAcpConfig::Vera(config);
        self
    }

    /// Configure Vera ACP when LCD and gRPC use distinct endpoints.
    #[cfg(feature = "vera")]
    pub fn with_vera_lcd(mut self, config: VeraConfig, lcd_address: impl Into<String>) -> Self {
        self.document_acp = DocumentAcpConfig::VeraWithLcd {
            config,
            lcd_address: lcd_address.into(),
        };
        self
    }

    /// Use an identity already registered in DefraDB's process-local signing registry.
    ///
    /// The caller must register the signer before calling [`NodeBuilder::build`].
    /// Registered identities may be backed by exportable private key bytes or by a
    /// remote signer such as a host Secure Enclave adapter.
    pub fn with_node_identity_did(mut self, did: impl Into<String>) -> Self {
        self.node_identity_did = Some(did.into());
        self
    }

    /// Enable Node Access Control (NAC) with the configured node identity as owner.
    ///
    /// NAC is disabled by default. When enabled, DB-layer node operations are
    /// gated by the NAC policy; the node's own identity always retains full
    /// access (it is the owner). Requires [`NodeBuilder::with_node_identity_did`]
    /// to be set with an identity backed by local key bytes, so the node DID can
    /// own the policy — [`NodeBuilder::build`] fails otherwise.
    pub fn with_node_acp_enabled(mut self) -> Self {
        self.node_acp_enabled = true;
        self
    }

    /// Set the query execution timeout.
    ///
    /// `Duration::ZERO` disables query execution timeouts.
    pub fn with_query_timeout(mut self, timeout: Duration) -> Self {
        self.query_timeout = Some(timeout);
        self
    }

    /// Set GraphQL parsing and filter evaluation limits.
    pub fn with_query_limits(mut self, limits: QueryLimits) -> Self {
        self.query_limits = limits;
        self
    }

    /// Enable or disable the introspection schema cache. Enabled by default.
    pub fn with_introspection_cache(mut self, enabled: bool) -> Self {
        self.introspection_cache = Some(enabled);
        self
    }

    /// Enable the HTTP GraphQL server.
    #[cfg(feature = "http")]
    pub fn with_http(mut self, config: HttpConfig) -> Self {
        self.http_config = Some(config);
        self
    }

    /// Enable Iroh/QUIC P2P replication.
    ///
    /// Requires the `p2p` feature, which implies `native`. Does not compile
    /// libp2p; that stack lives on CLI, FFI, and `embedded`.
    #[cfg(feature = "p2p")]
    pub fn with_p2p(mut self, config: P2PConfig) -> Self {
        self.p2p_config = Some(config);
        self
    }

    /// Hand a pre-built telemetry handle to the node so it owns shutdown.
    ///
    /// The caller is responsible for calling [`telemetry::init`] and
    /// composing the bridge layer onto their `tracing` subscriber — this
    /// method just transfers ownership of the lifecycle handle so the node
    /// flushes providers when it shuts down. Because the handle is moved,
    /// calling this twice is a compile error rather than a silent overwrite.
    ///
    /// For a one-call ergonomic version, callers can write
    /// `.with_telemetry(telemetry::init(cfg)?.0)` — `init` requires a Tokio
    /// runtime and returns `(TelemetryHandle, SdkTracer)`.
    #[cfg(feature = "otel")]
    pub fn with_telemetry(mut self, handle: TelemetryHandle) -> Self {
        self.telemetry_handle = Some(handle);
        self
    }

    /// A custom CRDT type, which a field takes with `@crdt(custom: "<name>")`;
    /// `crdt.ctype()` is `CType::Custom` of that name. The type implements
    /// [`crdt::FieldCrdt`] and passes `crdt::field::testing::check_field_crdt`.
    ///
    /// The type belongs to this node alone: another node in the process has
    /// only the types its own builder registers. A node without the type
    /// keeps and replicates definitions naming it, leaves the field out of
    /// its queries, and parks the field's replicated blocks; a later build
    /// with the type merges them. Registering two types under one name fails
    /// [`build`](Self::build).
    pub fn with_crdt<T>(mut self, crdt: T) -> Self
    where
        T: crdt::FieldCrdt + 'static,
    {
        self.crdts.push(crdt::Registration::new(crdt));
        self
    }

    /// Build and start the embedded DefraDB node.
    pub async fn build(mut self) -> Result<EmbeddedNode> {
        let crdts = crdt::Crdts::new(std::mem::take(&mut self.crdts))?;
        let node_identity_did = self.node_identity_did.clone();
        let node_identity_config = node_identity_did
            .as_deref()
            .map(resolve_registered_node_identity)
            .transpose()?;
        let node_p2p_identity = match (node_identity_did.as_deref(), node_identity_config.as_ref())
        {
            (Some(did), Some(config)) => {
                local_raw_identity_from_registered_config(did, config)?.map(Arc::new)
            }
            _ => None,
        };

        // 1. Event bus
        let event_bus: Arc<dyn events::Bus> = Arc::new(events::ChannelBus::default());

        let db_options = {
            let mut options = db::DbOptions::default();
            if let Some(url) = self.embedding_url.as_ref() {
                options = options.with_embedding_url(url.clone());
            }
            if let Some(model) = self.embedding_model.as_ref() {
                options = options.with_embedding_model(model.clone());
            }
            if let Some(api_key) = self.embedding_api_key.as_ref() {
                options = options.with_embedding_api_key(api_key.clone());
            }
            if let Some(retries) = self.max_txn_retries {
                options = options.with_max_txn_retries(retries);
            }
            if let Some(identity) = node_p2p_identity.as_ref() {
                options = options.with_node_identity_arc(identity.clone());
            }
            options
        };

        // 2. Extract configs before moving self
        #[cfg(feature = "http")]
        let http_config = self.http_config;
        #[cfg(feature = "http")]
        let transaction_cleanup_config = match http_config.as_ref() {
            Some(config) if config.transaction_idle_timeout.is_zero() => None,
            Some(config) => {
                if config.transaction_cleanup_interval.is_zero() {
                    return Err(Error::ZeroCleanupInterval);
                }

                Some(TransactionCleanupConfig {
                    max_idle_age: config.transaction_idle_timeout,
                    sweep_interval: config.transaction_cleanup_interval,
                })
            }
            None => None,
        };
        #[cfg(feature = "p2p")]
        let p2p_config = self.p2p_config;
        let query_timeout = self.query_timeout;
        let query_limits = self.query_limits;
        let introspection_cache = self.introspection_cache.unwrap_or(true);
        let node_acp_enabled = self.node_acp_enabled;

        // Telemetry handle (if any) was moved into the builder via
        // `with_telemetry`. Threaded through `StoreBuildArgs` to the
        // `EmbeddedNode` construction site so its Drop / explicit
        // `shutdown()` runs at the right point.
        #[cfg(feature = "otel")]
        let telemetry_handle: Option<TelemetryHandle> = self.telemetry_handle;

        // The node's network runtime comes first, so a failure to start it
        // leaves no network work behind.
        #[cfg(feature = "http")]
        let serves_http = http_config.is_some();
        #[cfg(not(feature = "http"))]
        let serves_http = false;
        #[cfg(feature = "p2p")]
        let runs_p2p = p2p_config.is_some();
        #[cfg(not(feature = "p2p"))]
        let runs_p2p = false;
        let network = NodeEdge::start(
            self.edge,
            node_identity_did.is_some() || serves_http || runs_p2p,
        )?;

        // The data path's worker pool comes before the store, which places
        // its compaction threads by the pool's plan; the network runtime's
        // timer, when the node has one, runs the pool's stuck-poll monitor.
        let pool = storage::start_pool(
            self.workers.clone().unwrap_or_default(),
            network
                .as_ref()
                .map(|network| network.edge().pool_monitor()),
        )
        .map_err(Error::WorkerPoolStart)?;
        let db_options = db_options.with_pool(pool.clone());

        // 3. Storage backend + database
        let node = if let Some(path) = self.data_path {
            std::fs::create_dir_all(&path)?;

            let persistent_args = PersistentStoreBuildArgs {
                network,
                document_acp_config: self.document_acp.clone(),
                db_options: db_options.clone(),
                event_bus,
                node_identity_did: node_identity_did.clone(),
                #[cfg(feature = "p2p")]
                node_p2p_identity: node_p2p_identity.clone(),
                node_acp_enabled,
                query_timeout,
                query_limits,
                introspection_cache,
                #[cfg(feature = "http")]
                transaction_cleanup_config,
                #[cfg(feature = "p2p")]
                p2p_config,
                #[cfg(feature = "otel")]
                telemetry_handle,
            };

            tracing::info!(
                storage_backend = "regolith",
                data_path = %path.display(),
                "embedded node starting"
            );
            let opts = with_at_rest_keys(
                resolve_regolith_options(self.regolith_options.clone(), self.storage_durability)
                    .placed_on(&pool)
                    .with_crdts(crdts),
                self.at_rest_encryption_key,
                &self.previous_at_rest_encryption_keys,
            )?;
            let store =
                storage::RegolithStore::open_with_options(&path, opts).map_err(Error::StoreOpen)?;

            Self::build_with_persistent_store_arc(Arc::new(store), persistent_args).await?
        } else {
            tracing::info!(
                storage_backend = "memory",
                "embedded node starting (ephemeral, no data_path)"
            );
            let store = Arc::new(
                storage::RegolithStore::open_with_options(
                    "memory",
                    with_at_rest_keys(
                        storage::RegolithStoreOptions::memory()
                            .placed_on(&pool)
                            .with_crdts(crdts),
                        self.at_rest_encryption_key,
                        &self.previous_at_rest_encryption_keys,
                    )?,
                )
                .map_err(Error::MemoryStoreOpen)?,
            );

            Self::build_with_store(
                store,
                StoreBuildArgs {
                    network,
                    persistence: node_acp::Persistence::Memory,
                    document_acp_config: self.document_acp,
                    db_options,
                    event_bus,
                    node_identity_did: node_identity_did.clone(),
                    #[cfg(feature = "p2p")]
                    node_p2p_identity,
                    node_acp_enabled,
                    query_timeout,
                    query_limits,
                    introspection_cache,
                    #[cfg(feature = "http")]
                    transaction_cleanup_config,
                    #[cfg(feature = "p2p")]
                    p2p_config,
                    #[cfg(feature = "otel")]
                    telemetry_handle,
                },
            )
            .await?
        };

        // 4. Spawn HTTP server if configured
        #[cfg(feature = "http")]
        if let Some(http_cfg) = http_config {
            let network = node.network.as_ref().ok_or(Error::HttpNeedsNetwork)?;
            let server_config = defra_http::ServerConfig {
                address: http_cfg.address,
                request_timeout: timeout_secs(http_cfg.request_timeout),
                query_limits,
                ..Default::default()
            };
            let server = defra_http::Server::from_arc_with_config(
                node.runner.clone(),
                server_config,
                network.edge().clone(),
            )
            .with_event_bus_arc(node.event_bus.clone())
            .with_pool(node.pool.clone())
            .with_schema_arc(Arc::new(HttpSchemaOps {
                schema_ops: Arc::clone(&node.schema_ops),
                context: None,
            }))
            .with_collection_versions_arc(Arc::clone(&node.collection_version_ops));

            let server = if let Some(did) = node_identity_did.as_ref() {
                server.with_node_identity_did(did.clone())
            } else {
                server
            };

            #[cfg(feature = "p2p")]
            let server = if let Some(p2p) = node.p2p_ops.as_ref() {
                server.with_p2p_arc(Arc::clone(p2p))
            } else {
                server
            };

            let server = match http_cfg.extra_routes {
                Some(extra) => server.with_extra_routes(extra),
                None => server,
            };
            let server = match http_cfg.router_transform {
                Some(transform) => server.with_router_transform(transform),
                None => server,
            };
            let addr = http_cfg.address;
            let bound = http_cfg.listener;
            // The accept loop is a service of the node's scope: shutting the
            // node down stops it accepting.
            let service = network.edge().spawn_service(async move {
                let served = match bound {
                    Some(listener) => server.serve_on(listener).await,
                    None => server.run().await,
                };
                if let Err(e) = served {
                    tracing::error!(error = %e, address = %addr, "HTTP server exited with error");
                }
            });
            service.detach();
            tracing::info!(address = %addr, "HTTP server started");
        }

        Ok(node)
    }

    async fn build_with_persistent_store_arc<S: storage::corekv::Store + 'static>(
        store: Arc<S>,
        args: PersistentStoreBuildArgs,
    ) -> Result<EmbeddedNode> {
        let PersistentStoreBuildArgs {
            network,
            document_acp_config,
            db_options,
            event_bus,
            node_identity_did,
            #[cfg(feature = "p2p")]
            node_p2p_identity,
            node_acp_enabled,
            query_timeout,
            query_limits,
            introspection_cache,
            #[cfg(feature = "http")]
            transaction_cleanup_config,
            #[cfg(feature = "p2p")]
            p2p_config,
            #[cfg(feature = "otel")]
            telemetry_handle,
        } = args;

        Self::build_with_store(
            store,
            StoreBuildArgs {
                network,
                persistence: node_acp::Persistence::Persistent,
                document_acp_config,
                db_options,
                event_bus,
                node_identity_did,
                #[cfg(feature = "p2p")]
                node_p2p_identity,
                node_acp_enabled,
                query_timeout,
                query_limits,
                introspection_cache,
                #[cfg(feature = "http")]
                transaction_cleanup_config,
                #[cfg(feature = "p2p")]
                p2p_config,
                #[cfg(feature = "otel")]
                telemetry_handle,
            },
        )
        .await
    }

    async fn build_with_store<S: storage::corekv::Store + 'static>(
        store: Arc<S>,
        args: StoreBuildArgs,
    ) -> Result<EmbeddedNode> {
        let StoreBuildArgs {
            network,
            persistence,
            document_acp_config,
            db_options,
            event_bus,
            node_identity_did,
            #[cfg(feature = "p2p")]
            node_p2p_identity,
            node_acp_enabled,
            query_timeout,
            query_limits,
            introspection_cache,
            #[cfg(feature = "http")]
            transaction_cleanup_config,
            #[cfg(feature = "p2p")]
            p2p_config,
            #[cfg(feature = "otel")]
            telemetry_handle,
        } = args;

        let node_query_identity = node_identity_did
            .as_ref()
            .map(|did| identity::Did::new(did.clone()))
            .transpose()
            .map_err(Error::InvalidNodeIdentityDid)?;

        let embedding_config = db_options.embedding_config();
        #[cfg(not(target_arch = "wasm32"))]
        let transaction_stats = store.transaction_stats_handle();

        // Open database
        let mut database = db::DB::open_from_arc_with_options(store.clone(), db_options)
            .await
            .map_err(Error::DatabaseOpen)?;

        // Wire event bus so mutations publish events
        database.set_event_bus(event_bus.clone());
        if let Some(network) = network.as_ref() {
            database.set_network(network.edge().clone());
        }
        let database = Arc::new(database);
        let closer = close_database(&database);

        // Node Access Control: only installed when explicitly enabled. The node
        // identity owns the policy, so DB-layer node operations run by the node
        // itself are always allowed (the node-DID bypass in check_node_access).
        if node_acp_enabled {
            let owner = node_identity_did
                .as_deref()
                .ok_or(Error::NodeAcpNeedsIdentity)
                .and_then(|did| identity::Did::new(did).map_err(Error::InvalidNodeAcpIdentity))?;
            if database.node_did().as_ref() != Some(&owner) {
                return Err(Error::NodeAcpNeedsLocalKey);
            }
            let nac_store = Arc::new(acp::PersistentZanzibarStore::from_store(store.clone()));
            let nac_config = db::NacConfig::new().with_enabled().with_dev_mode();
            let nac_manager = Arc::new(db::NacManager::new(nac_store, nac_config));
            nac_manager
                .initialize(Some(&owner))
                .await
                .map_err(Error::NodeAcpEnable)?;
            database.set_nac_manager(nac_manager);
        }

        let acp_setup = node_acp::create_document_acp(
            store.clone(),
            persistence,
            &document_acp_config,
            network.as_ref().map(NodeEdge::edge),
        )
        .await?;
        let document_acp = acp_setup.document_acp.clone();
        #[cfg(feature = "vera")]
        let remote_acp = acp_setup.vera_acp.is_some();
        #[cfg(not(feature = "vera"))]
        let remote_acp = false;
        let _strict_replicated_doc_access = remote_acp;

        // P2P setup (affects mutator choice)
        #[cfg(feature = "p2p")]
        let mut p2p_result = if let Some(p2p_cfg) = p2p_config {
            let network = network.as_ref().ok_or(Error::P2pNeedsNetwork)?;
            Some(
                p2p_runtime::setup_p2p(
                    store.clone(),
                    database.clone(),
                    event_bus.clone(),
                    &p2p_cfg,
                    node_p2p_identity,
                    document_acp.clone(),
                    _strict_replicated_doc_access,
                    network.edge(),
                )
                .await?,
            )
        } else {
            None
        };

        // Choose mutator: BroadcastMutator if P2P, AutoCommitMutator otherwise
        #[cfg(feature = "p2p")]
        let mutator: Arc<dyn query::DocMutator> = if let Some(ref p2p) = p2p_result {
            p2p.mutator.clone()
        } else {
            Arc::new(db::AutoCommitMutator::new(database.clone()))
        };
        #[cfg(not(feature = "p2p"))]
        let mutator: Arc<dyn query::DocMutator> =
            Arc::new(db::AutoCommitMutator::new(database.clone()));

        // Query runner components
        let fetcher = db::LensedAutoCommitFetcher::new(database.clone());
        let provider: Arc<dyn query::CollectionProvider> =
            db::DbCollectionProvider::new_arc(database.clone());

        #[cfg(feature = "p2p")]
        let txn_broadcaster: Option<Arc<dyn db::event::emission::TxnBroadcaster>> =
            p2p_result.as_ref().map(|r| r.txn_broadcaster.clone());
        #[cfg(not(feature = "p2p"))]
        let txn_broadcaster: Option<Arc<dyn db::event::emission::TxnBroadcaster>> = None;

        let registry = Arc::new(match txn_broadcaster {
            Some(b) => db::DbTransactionRegistry::with_broadcaster(database.clone(), b),
            None => db::DbTransactionRegistry::new(database.clone()),
        });

        #[cfg(feature = "http")]
        let txn_cleanup_task = transaction_cleanup_config.map(|cleanup| {
            tracing::info!(
                max_idle_age_secs = cleanup.max_idle_age.as_secs(),
                sweep_interval_secs = cleanup.sweep_interval.as_secs(),
                "Transaction idle cleanup worker enabled"
            );
            registry.start_stale_transaction_cleanup(cleanup.max_idle_age, cleanup.sweep_interval)
        });

        // Build the KMS once document ACP exists (same ordering as the CLI
        // runtime and crates/embedded/src/node.rs): blockstore-backed key
        // store, NAC/DAC release policy, and the pubsub transport created in
        // setup_p2p. Without this, encrypted blocks replicate but no peer can
        // ever fetch the DEK, so encrypted documents never become readable on
        // replicas.
        #[cfg(feature = "p2p")]
        if let Some(p2p) = p2p_result.as_mut() {
            // Blockstore-backed KeyStore: the KMS serves DEKs for ANY
            // encrypted write by reading the node's durable
            // encstore->blockstore, not a RAM-only map. The DB owns the
            // blockstore Arc (set_kms_blockstore) so the adapter can hold a
            // Weak and avoid the lock-pinning cycle (#976).
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
            if let Some(nac) = database.nac_manager() {
                policy.set_node_acp(Arc::new(db::DbNodeAcpRead::new(nac)));
            }

            // Node identity used for cross-peer DEK requests. Anonymous
            // nodes use a stable placeholder DID.
            let kms_node_did = database.node_did().unwrap_or_else(|| {
                identity::Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")
                    .expect("static anonymous DID parses")
            });

            let kms_service: Arc<dyn kms::KmsService> = Arc::new(kms::DefraKms::with_inputs(
                key_store,
                vec![p2p.kms_transport.clone()],
                policy as Arc<dyn kms::AccessPolicy>,
                Arc::new(db::DbBlockDocIDResolver::new(database.clone())),
                kms_node_did,
                network
                    .as_ref()
                    .map(|network| kms::KmsInputs::on(network.edge()))
                    .unwrap_or_default(),
            ));

            // Bind this node's transport peer id into the KMS so served
            // ECIES replies carry the correct AAD peer id.
            kms_service.set_local_peer_id(p2p.local_peer_id.clone());

            // Serve handler with a Weak to break the transport<->kms Arc
            // cycle (transport holds handler -> kms -> transports ->
            // transport).
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
            p2p.kms_transport.install_handler(Arc::new(KmsServeHandler {
                kms: Arc::downgrade(&kms_service),
            }));

            if let Some(wire_kms) = p2p.wire_kms.take() {
                wire_kms(kms_service.clone());
            }

            // Wire the KMS into the DB-held read/write path.
            database.set_kms(kms_service);
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

        // GraphQL requests run on the database's worker pool, routed there
        // once at the boundary.
        let runner = query::pooled::PooledExecutor::wrap(Arc::new(query_runner), pool.as_ref());
        #[cfg(all(feature = "vera", not(target_arch = "wasm32")))]
        let redrive = acp_setup.redrive;
        #[cfg(feature = "vera")]
        let policy_lookup =
            acp_ops::PolicyLookup::new(acp_setup.local_zanzibar_store, acp_setup.vera_acp);
        #[cfg(not(feature = "vera"))]
        let policy_lookup = acp_ops::PolicyLookup::new(acp_setup.local_zanzibar_store);
        let schema_ops: Arc<dyn SchemaOps> = Arc::new(db_impls::DbSchemaOps::new(
            database.clone(),
            query_limits,
            document_acp.clone(),
            policy_lookup.clone(),
        ));
        #[cfg(feature = "http")]
        let collection_version_ops: Arc<
            dyn defra_http::router::CollectionVersionOperations,
        > = Arc::new(db_impls::DbCollectionVersionOps::new(Arc::clone(&database)));
        let acp_ops: Arc<dyn acp_ops::AcpOps> = Arc::new(acp_ops::DbAcpOps::new(
            database.clone(),
            document_acp.clone(),
            policy_lookup,
            event_bus.clone(),
        ));
        // Started last: nothing after this point fails, so a failed build
        // never leaves a task holding the store open.
        #[cfg(not(target_arch = "wasm32"))]
        let background_tasks = BackgroundTasks::new([
            database.clone().start_vector_rebuild_task(),
            database.clone().start_downsample_task(),
        ]);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(replay) = database.clone().start_parked_replay() {
            background_tasks.0.push(replay);
        }
        #[cfg(all(feature = "vera", not(target_arch = "wasm32")))]
        if let Some(start_redrive) = redrive {
            background_tasks.0.push(start_redrive(database.pool()));
        }
        let block_ops: Arc<dyn BlockOps> = Arc::new(db_impls::DbBlockOps::new(
            database,
            document_acp.clone(),
            registry,
            node_query_identity.clone(),
        ));

        #[cfg(feature = "p2p")]
        let (p2p_ops, p2p_lifecycle) = match p2p_result {
            Some(result) => (Some(result.ops), result.lifecycle),
            None => (None, None),
        };

        Ok(EmbeddedNode {
            runner,
            event_bus,
            schema_ops,
            #[cfg(feature = "http")]
            collection_version_ops,
            block_ops,
            acp_ops,
            document_acp,
            embedding_config,
            node_identity_did,
            node_query_identity,
            network,
            close_database: closer,
            pool,
            remote_acp,
            #[cfg(not(target_arch = "wasm32"))]
            transaction_stats,
            #[cfg(feature = "http")]
            txn_cleanup_task: single_slot(txn_cleanup_task),
            #[cfg(not(target_arch = "wasm32"))]
            background_tasks,
            #[cfg(feature = "p2p")]
            p2p_ops,
            #[cfg(feature = "p2p")]
            p2p_lifecycle,
            #[cfg(feature = "otel")]
            telemetry: single_slot(telemetry_handle),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use defra_core::signing::{RemoteSigner, SigningConfig, SigningKeyType};
    use regolith::sync::Mutex;

    use super::EmbeddedNode;

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

                let node = EmbeddedNode::builder()
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
                    .get(&schema_url)
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
                    .delete(&url)
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
    fn node_builder_accepts_regolith_options() {
        let builder = EmbeddedNode::builder()
            .with_regolith_options(storage::RegolithStoreOptions::embedded());

        assert_eq!(
            builder
                .regolith_options
                .as_ref()
                .map(|options| options.engine.get_write_buffer_size()),
            Some(256 * 1024)
        );
    }

    #[test]
    fn regolith_options_default_to_server_sizing() {
        let resolved =
            super::resolve_regolith_options(None, storage::backends::DurabilityMode::Immediate);
        let server = storage::RegolithStoreOptions::default();

        assert_eq!(
            resolved.engine.get_write_buffer_size(),
            server.engine.get_write_buffer_size()
        );
        assert_eq!(
            resolved.engine.get_block_cache_size(),
            server.engine.get_block_cache_size()
        );
    }

    #[test]
    fn explicit_regolith_options_survive_and_durability_stays_with_the_builder() {
        let resolved = super::resolve_regolith_options(
            Some(storage::RegolithStoreOptions::embedded()),
            storage::backends::DurabilityMode::Eventual,
        );
        let expected = storage::RegolithStoreOptions::embedded()
            .with_durability(storage::backends::DurabilityMode::Eventual);

        assert_eq!(resolved.engine.get_write_buffer_size(), 256 * 1024);
        assert_eq!(resolved.engine.get_block_cache_size(), 0);
        assert_eq!(format!("{resolved:?}"), format!("{expected:?}"));
    }

    #[test]
    fn node_builder_accepts_query_timeout() {
        let timeout = std::time::Duration::from_secs(45);
        let builder = EmbeddedNode::builder().with_query_timeout(timeout);

        assert_eq!(builder.query_timeout, Some(timeout));
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

                let error = match EmbeddedNode::builder()
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

                let node = EmbeddedNode::builder()
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
mod router_transform_tests;

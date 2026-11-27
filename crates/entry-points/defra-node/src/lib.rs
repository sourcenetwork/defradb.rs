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
//!
//! ## Who a call acts as, and in which transaction
//!
//! [`Node`] acts as the node's own principal. [`Node::with_identity`] and
//! [`Node::anonymous`] scope it to a caller, and [`Node::new_txn`] or
//! [`Node::with_transaction`] to an explicit transaction, which offers only
//! the operations that run in one:
//!
//! ```no_run
//! # async fn example(node: &defra_node::Node, user: identity::Did) -> defra_node::Result<()> {
//! node.add_collection("type User { name: String }").await?;
//! let txn = node.with_identity(user).new_txn(false).await?;
//! txn.add_view("User { name }", "type UserView { name: String }", None)
//!     .await?;
//! txn.commit().await
//! # }
//! ```

#[cfg(feature = "p2p")]
mod access_hooks;
mod acp_ops;
#[cfg(feature = "vera")]
mod acp_provider;
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
mod embedded_signer;
#[cfg(feature = "embedded-vera")]
mod embedded_vera;
#[cfg(feature = "embedded-vera")]
mod embedded_vera_config;
#[cfg(feature = "embedded-vera")]
mod embedded_vera_switch;
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
mod node_core;
mod node_identity;
#[cfg(test)]
mod node_query_identity_tests;
#[cfg(test)]
mod on_pool_tests;
mod ops;
#[cfg(feature = "vera")]
mod owed_drainer;
#[cfg(test)]
mod storage_open_tests;
pub use node_core::IndexListing;
pub use ops::EncryptedIndexListing;
#[cfg(any(feature = "p2p", feature = "libp2p"))]
mod p2p_runtime;
#[cfg(any(feature = "p2p", feature = "libp2p"))]
mod peer_key;
mod peer_keyring;
#[cfg(feature = "pinc")]
mod pinc_runtime;
mod request_path;
mod scope;
mod signed_query;
mod signer_credential;
mod start_peer_key;
mod transaction;
#[cfg(feature = "vera")]
mod vera_config;
pub mod version;

use std::sync::Arc;
use std::time::Duration;

use defra_core::RequestContext;

#[cfg(feature = "p2p")]
pub use access_hooks::AccessHooks;
/// Re-exported so callers can name `Arc<dyn DocumentACP>` (the type
/// [`Node::document_acp`] returns) without depending on `acp` directly.
pub use acp::nac::NodePermission;
pub use acp::ReplicaGrant;
pub use acp::{AssignmentPosition, AssignmentQuery, CertifiedAssignments};
pub use acp::{DocumentACP, DocumentPermission, Identity as AcpIdentity, PolicyNonce};
pub use acp_ops::transactional::RelationshipChange;
#[cfg(feature = "vera")]
pub use acp_provider::DocumentAcpProvider;
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
#[cfg(feature = "pinc")]
pub use config::PincConfig;
#[cfg(any(feature = "p2p", feature = "libp2p"))]
pub use config::SyncTuning;
pub use config::{
    CommitGateHandle, DocumentAcpConfig, IndexBuildGateHandle, NodeConfig, NodeSigning,
    Persistence, ReplicationFilterHandle, SigningKey, TransportConfig,
};
#[cfg(feature = "p2p")]
pub use config::{P2PConfig, PeerKeySource};
/// The hook [`NodeBuilder::with_index_build_gate`] takes.
pub use db::index::build_gate::{GateWait, IndexBuildGate};
pub use db::merge::{ReplicaOwnership, ReplicaRegistration};
pub use db::{CommitGate, CommitWait};
#[cfg(feature = "managed-p2p")]
pub use defra_p2p_adapter::{
    ReplicationFilter, ReplicationFilters, ReplicatorPushOptions, ReplicatorPushOptionsState,
};
pub use dense_search::{DenseHybridSearchHit, DenseHybridSearchRequest, DenseHybridSearchResponse};
pub use embedded_signer::{EmbeddedSigner, EmbeddedSignerError};
#[cfg(feature = "embedded-vera")]
pub use embedded_vera_config::{
    EmbeddedVeraActor, EmbeddedVeraConfig, EmbeddedVeraDeployment, EmbeddedVeraDevice,
    EmbeddedVeraNode, EmbeddedVeraPolicy, EmbeddedVeraTrust,
};
pub use error::{Error, Result};
pub use events::EventName;
pub use keyring;
pub use keyring::Keyring;
pub use lens::{LensConfig, LensModule, TransformId};
#[cfg(feature = "managed-p2p")]
pub use managed_p2p::{ManagedP2PSystem, ShutdownHandle, TransportKind};
pub use node_builder::{NodeBuilder, StorageBackend, StoreProfile};
/// Runs a pinc hub, which pinc topics travel through.
#[cfg(feature = "pinc")]
pub use p2p::pinc::spawn_hub as spawn_pinc_hub;
pub use peer_keyring::PeerKeyring;
pub use query::owners::{DocumentAccessDecision, DocumentAccessMode};
pub use query::QueryLimits;
pub use query::{QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};
pub use schema::CollectionVersion;
pub use scope::{ActingFor, ScopedNode, TxnNode};
pub use signer_credential::SignerCredential;
pub use start_peer_key::StartPeerKey;
pub use telemetry::{ConflictMetricsSnapshot, RetryLayerSnapshot};
#[cfg(feature = "otel")]
pub use telemetry::{TelemetryConfig, TelemetryHandle};
#[cfg(feature = "vera")]
pub use vera_config::{VeraActor, VeraConfig};
/// The embedded Vera host's types: a deployment's key material, the
/// account's devices, term certificates and what a switch reports.
#[cfg(feature = "embedded-vera")]
pub use vera_embedded;
pub use zeroize::Zeroizing;

use network::NodeEdge;

#[cfg(feature = "http")]
struct HttpSchemaOps {
    schema_ops: db_impls::DbSchemaOps,
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
            schema_ops: self.schema_ops.clone(),
            context: Some(context.clone()),
        }))
    }

    async fn add_collection(
        &self,
        sdl: &str,
    ) -> Result<Vec<CollectionVersion>, defra_http::router::CollectionRefusal> {
        self.schema_ops
            .add_collection(
                &RequestContext::bound_or_ambient(self.context.as_ref()),
                sdl,
            )
            .await
            .map_err(|error| match error {
                Error::AddCollection(refused) => refused.into(),
                other => other.to_string().into(),
            })
    }
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
    #[cfg(feature = "http")]
    rest: Arc<dyn query::rest::RestOperations>,
    event_bus: Arc<dyn events::Bus>,
    schema_ops: db_impls::DbSchemaOps,
    block_ops: db_impls::DbBlockOps,
    acp_ops: Arc<dyn acp_ops::AcpOps>,
    document_acp: Arc<dyn acp::DocumentACP>,
    embedding_config: db::EmbeddingClientConfig,
    node_identity_did: Option<String>,
    /// Whether the node signs the blocks it writes with its identity; false
    /// for an identity-only node ([`NodeBuilder::with_identity_only`]).
    node_signs: bool,
    /// The identity the node acts as when a caller names none: its own DID
    /// when it signs with it, `None` on an identity-only node. It is the
    /// default query actor, the principal of [`Node::node_context`] and so the
    /// owner of the transactions the node begins.
    node_query_identity: Option<identity::Did>,
    /// The node's share of the network runtime; `None` when the node has
    /// no network work and was given no runtime.
    network: Option<NodeEdge>,
    /// The database's worker pool: the node's data-path calls run there.
    pool: Option<workers::Pool>,
    /// The Vera node this node runs for document ACP. Shutdown stops it; a
    /// node dropped without [`Node::shutdown`] drops it here, which tells it
    /// to stop and waits for nothing.
    #[cfg(feature = "embedded-vera")]
    embedded_vera: Option<vera_embedded::HostedNode>,
    #[cfg(feature = "embedded-vera")]
    embedded_vera_trust: Option<EmbeddedVeraTrust>,
    #[cfg(feature = "embedded-vera")]
    embedded_vera_rpc_addr: Option<std::net::SocketAddr>,
    /// Document ACP is a remote provider: its calls reach the network, so
    /// they stay where they are made.
    remote_acp: bool,
    #[cfg(not(target_arch = "wasm32"))]
    transaction_stats: Option<storage::TransactionStatsHandle>,
    /// Vector index rebuild, downsample view refresh, parked-block replay,
    /// the Vera redrive and the HTTP transaction sweep. Each holds the
    /// database open until aborted; dropping the set aborts them.
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
    /// Keeps the key the node made or loaded in the signing store while the
    /// node lives. Declared last, so it drops last: a node dropped without
    /// [`Node::shutdown`] keeps its key until its workers, background tasks
    /// and P2P are gone.
    _signing_key_hold: Option<defra_core::signing::IdentityHold>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
struct TransactionCleanupConfig {
    max_idle_age: Duration,
    sweep_interval: Duration,
}

#[cfg(feature = "otel")]
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

trait VerifiedClientGraph {
    fn signer_did(&self) -> &str;
    fn doc_id(&self) -> &str;
    fn matches_mutation(&self, mutation: &query::mapper::Mutation) -> bool;
    fn signing_config_for_execution(
        &self,
    ) -> (
        defra_core::signing::SigningConfig,
        db::block::builder::VerifiedSignatureReceipt,
    );
}

impl VerifiedClientGraph for db::block::builder::VerifiedClientCreate {
    fn signer_did(&self) -> &str {
        self.signer_did()
    }
    fn doc_id(&self) -> &str {
        self.doc_id()
    }
    fn matches_mutation(&self, mutation: &query::mapper::Mutation) -> bool {
        self.matches_mutation(mutation)
    }
    fn signing_config_for_execution(
        &self,
    ) -> (
        defra_core::signing::SigningConfig,
        db::block::builder::VerifiedSignatureReceipt,
    ) {
        self.signing_config_for_execution()
    }
}

impl VerifiedClientGraph for db::block::builder::VerifiedClientMutation {
    fn signer_did(&self) -> &str {
        self.signer_did()
    }
    fn doc_id(&self) -> &str {
        self.doc_id()
    }
    fn matches_mutation(&self, mutation: &query::mapper::Mutation) -> bool {
        self.matches_mutation(mutation)
    }
    fn signing_config_for_execution(
        &self,
    ) -> (
        defra_core::signing::SigningConfig,
        db::block::builder::VerifiedSignatureReceipt,
    ) {
        self.signing_config_for_execution()
    }
}

impl Node {
    /// Start building a new node.
    pub fn builder() -> NodeBuilder {
        NodeBuilder::default()
    }

    /// Execute a GraphQL query or mutation.
    ///
    /// A configured node identity is used as the document ACP actor, except on
    /// an identity-only node, where the request acts as no one. Prepared
    /// requests with an explicit identity retain that actor instead.
    pub async fn execute(&self, query_str: &str) -> QueryResponse {
        self.execute_request_once(QueryRequest::new(query_str))
            .await
    }

    /// Execute a prepared query request once, resolving external inputs between attempts.
    ///
    /// Storage conflicts are returned to the caller. Eligible implicit mutations
    /// retry inside the engine-owned transaction boundary.
    ///
    /// A caller's key on the request ([`EmbeddedSigner::attach`]) signs in
    /// place of the node's. The request must name that key's DID as its actor,
    /// native ACP authorizes that actor, and the node's principal (its DID, or
    /// no one on an identity-only node) stays the transaction principal. This
    /// trusted embedded capability does not authenticate an HTTP caller.
    pub async fn execute_request(&self, request: QueryRequest) -> QueryResponse {
        self.execute_prepared_request(request, None).await
    }

    async fn execute_request_once(&self, request: QueryRequest) -> QueryResponse {
        self.execute_prepared_request(request, None).await
    }

    async fn execute_prepared_request(
        &self,
        request: QueryRequest,
        txn_handle: Option<TransactionHandle>,
    ) -> QueryResponse {
        self.execute_admitted_request(request, txn_handle, Admit::Request)
            .await
    }

    /// [`Self::execute_prepared_request`], admitted as `admit` says.
    pub(crate) async fn execute_admitted_request(
        &self,
        request: QueryRequest,
        txn_handle: Option<TransactionHandle>,
        admit: Admit,
    ) -> QueryResponse {
        self.request_path()
            .execute(request, txn_handle, admit)
            .await
    }

    /// This node's request path, borrowed.
    fn request_path(&self) -> request_path::RequestPath<'_> {
        request_path::RequestPath {
            runner: &self.runner,
            node_signs: self.node_signs,
            node_identity_did: self.node_identity_did.as_deref(),
            node_query_identity: self.node_query_identity.as_ref(),
            requests: self.network.as_ref().map(NodeEdge::requests),
        }
    }

    /// This node's request path, owned, for work that outlives a borrow of
    /// the node, such as a backup import's driver.
    pub(crate) fn owned_request_path(&self) -> request_path::OwnedRequestPath {
        request_path::OwnedRequestPath {
            runner: Arc::clone(&self.runner),
            node_signs: self.node_signs,
            node_identity_did: self.node_identity_did.clone(),
            node_query_identity: self.node_query_identity.clone(),
            requests: self
                .network
                .as_ref()
                .map(|network| network.requests().clone()),
        }
    }

    /// Runs in-process work that may write as a task of the node's request
    /// scope, as a signed query runs: admitted and counted until it ends, with
    /// no deadline of its own, so shutdown drains it before P2P stops
    /// recording head markers, refused once shutdown stops admitting requests,
    /// and run to its end if its caller stops waiting. A node with no network
    /// has no P2P, and runs it in place.
    async fn as_request<F>(&self, work: F) -> std::result::Result<F::Output, edge::EdgeError>
    where
        F: std::future::Future + defra_core::thread_bounds::MaybeSend + 'static,
        F::Output: defra_core::thread_bounds::MaybeSend + 'static,
    {
        self.admitted(Admit::Request, work).await
    }

    /// [`Self::as_request`], admitted as `admit` says.
    pub(crate) async fn admitted<F>(
        &self,
        admit: Admit,
        work: F,
    ) -> std::result::Result<F::Output, edge::EdgeError>
    where
        F: std::future::Future + defra_core::thread_bounds::MaybeSend + 'static,
        F::Output: defra_core::thread_bounds::MaybeSend + 'static,
    {
        self.request_path().admitted(admit, work).await
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
            .map_err(Error::worker_pool_call)?
    }

    /// Runs a document ACP call: on the pool when the ACP is the node's own
    /// store, where it is made when the ACP is a remote provider.
    async fn on_acp<F, Fut, T>(&self, make: F) -> Result<T>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        self.ops().on_acp(make).await
    }

    /// The context the node's own operations run in, and the owner of the
    /// transactions it begins: the node's principal (`node_principal`)
    /// acts in it. On a node that acts as its identity, DB-layer NAC checks
    /// resolve to the node itself (which holds full access). Without a node
    /// identity, or on an identity-only node, it is [`RequestContext::ambient`]
    /// and NAC checks the node's own operations as anonymous, as Go's node
    /// checks a call that names no identity.
    ///
    /// An embedder reads node-ACP-gated P2P information as the node through
    /// the unscoped node, as with [`Node::peer_info`].
    pub(crate) fn node_context(&self) -> RequestContext {
        let context = RequestContext::ambient();
        match self.node_principal() {
            Some(did) => context.with_identity(Some(did)),
            None => context,
        }
    }

    /// The identity the node acts as when a caller names none: its DID on a
    /// node that signs with it, no one on an identity-only node or a node
    /// without an identity.
    fn node_principal(&self) -> Option<String> {
        self.node_query_identity.as_ref().map(ToString::to_string)
    }

    /// The node's identity DID, when configured: the DID it signs with, or
    /// on an identity-only node the DID it keeps for P2P, the KMS and NAC
    /// without signing.
    pub fn node_identity_did(&self) -> Option<&str> {
        self.node_identity_did.as_deref()
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
        let (ops, context) = (self.schema_ops.clone(), self.node_context());
        let (collection, name) = (collection.to_owned(), name.to_owned());
        self.on_pool(move || async move {
            ops.ensure_ordinary_index(&context, &collection, &name, fields)
                .await
        })
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
        let (ops, version) = (self.schema_ops.clone(), version_id.to_owned());
        self.on_pool(move || async move { ops.get_collection_by_version_id(&version).await })
            .await
    }

    /// Return every collection version known to the node, active and inactive.
    pub async fn get_all_collection_versions(&self) -> Result<Vec<CollectionVersion>> {
        let ops = self.schema_ops.clone();
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

    async fn begin_owned_transaction(
        &self,
        context: RequestContext,
        readonly: bool,
    ) -> Result<transaction::OwnedTransaction, query::TransactionError> {
        let runner = Arc::clone(&self.runner);
        self.as_request(async move {
            let handle = runner.begin_txn_as(&context, readonly).await?;
            Ok(transaction::OwnedTransaction::new(runner, handle))
        })
        .await
        .unwrap_or_else(|error| Err(refused_transaction(error, "begun")))
    }

    /// Access the raw query executor for advanced use.
    ///
    /// Calling `execute` or `execute_in_txn` on this executor bypasses the
    /// node's signing context and default ACP identity. Use [`Self::execute`]
    /// or [`TxnNode::execute_request`] for document reads and writes.
    pub fn runner(&self) -> &Arc<dyn QueryExecutor> {
        &self.runner
    }

    /// Access the event bus directly.
    pub fn event_bus(&self) -> &Arc<dyn events::Bus> {
        &self.event_bus
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
    pub fn p2p(&self) -> Option<&dyn defra_p2p_adapter::P2POperations> {
        self.p2p.as_deref().map(|system| system.ops().as_ref())
    }

    /// Cloneable P2P operations handle for background tasks.
    #[cfg(feature = "managed-p2p")]
    pub fn p2p_arc(&self) -> Option<Arc<dyn defra_p2p_adapter::P2POperations>> {
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
    ///
    /// The embedded HTTP server decides when the node is built whether its
    /// edge routes check node ACP, as `defradb start` does at startup: they do
    /// once node ACP was enabled through the builder or by an earlier run on
    /// the same store. Enabling it here on a node built without it guards the
    /// database now and the edge routes from the next build.
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

    /// What another node of the same account needs to verify against the
    /// Vera deployment this node runs; `None` when it runs none.
    #[cfg(feature = "embedded-vera")]
    pub fn embedded_vera_trust(&self) -> Option<&EmbeddedVeraTrust> {
        self.embedded_vera_trust.as_ref()
    }

    /// The Vera node this node runs for document ACP, for a host that moves
    /// the deployment's active role between the account's devices
    /// ([`EmbeddedVeraDevice`]). Document ACP reaches whichever node a switch
    /// leaves running. `None` when this node runs no Vera node.
    #[cfg(feature = "embedded-vera")]
    pub fn embedded_vera(&self) -> Option<&EmbeddedVeraNode> {
        self.embedded_vera.as_ref()
    }

    /// The address bound at startup when the hosted Vera deployment serves JSON-RPC.
    #[cfg(feature = "embedded-vera")]
    pub fn embedded_vera_rpc_addr(&self) -> Option<std::net::SocketAddr> {
        self.embedded_vera_rpc_addr
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
        let peer_id =
            defra_p2p_adapter::TransportPeerId::new(peer_id).map_err(Error::InvalidPeerId)?;
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
        let peer_id =
            defra_p2p_adapter::TransportPeerId::new(peer_id).map_err(Error::InvalidPeerId)?;
        ops.deny_peer(&peer_id).await.map_err(Error::PeerRevoke)
    }

    /// Open a stream for `protocol` to the iroh peer `peer` and write
    /// `request` as its first message. The peer serves it if it registered
    /// `protocol` with [`NodeBuilder::with_stream_protocol`]. Finish the send
    /// half once the protocol has nothing more to send; dropping it
    /// unfinished resets the stream.
    ///
    /// `peer` is a peer ID or any address `connect_peer` takes. The stream
    /// rides the connection the two nodes share; with none open, this dials
    /// one just as `connect_peer` does, which then serves both nodes' streams.
    #[cfg(feature = "p2p")]
    pub async fn open_p2p_stream(
        &self,
        peer: &str,
        protocol: &p2p::iroh::StreamProtocol,
        request: Vec<u8>,
    ) -> Result<(p2p::iroh::ProtocolSendStream, p2p::iroh::ProtocolRecvStream)> {
        let transport = self
            .p2p
            .as_ref()
            .ok_or(Error::P2pDisabled)?
            .iroh_transport()
            .ok_or(Error::StreamProtocolsNeedIroh)?;
        let (peer_id, addrs) = p2p::iroh::parse_public_peer_addr(peer)?;
        Ok(transport
            .open_stream(&peer_id, addrs, protocol, request)
            .await?)
    }

    /// Gracefully stop background services owned by this node.
    ///
    /// **The node should not be used after this call.** When the `otel`
    /// feature is on, `shutdown` calls `TelemetryHandle::shutdown` which
    /// flushes and disables the global tracer provider. Subsequent calls
    /// that emit spans (e.g. `execute`, `add_collection`) go to a no-op
    /// tracer — they still work functionally, but observability is gone
    /// with no error surfaced. Drop the node after shutdown completes.
    ///
    /// Every step of the teardown runs; the database close error, if any, is
    /// returned once it has. With `embedded-vera`, a Vera node that did not
    /// stop is returned when the database closed cleanly; when both fail the
    /// database error is returned and the Vera one is logged. Only the first
    /// caller gets that outcome: a
    /// concurrent or later caller waits for the teardown and returns `Ok`.
    pub async fn shutdown(&self) -> Result<()> {
        // The first caller tears the node down; a concurrent caller waits for
        // that teardown, so `shutdown().await` always means it has finished.
        if self
            .shutdown_started
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            self.wait_for_shutdown().await;
            return Ok(());
        }
        tracing::info!("node shutdown: begin");

        #[cfg(not(target_arch = "wasm32"))]
        self.background_tasks.shutdown().await;

        // Requests first: no new signed query, in-process query, transaction
        // commit or HTTP request is admitted, and those in flight finish while
        // P2P still records their writes' durable head markers.
        if let Some(network) = &self.network {
            network.drain_requests().await;
        }

        // The transport and the tasks around it stop next and are awaited, so
        // none still reaches storage when the drain and the close come.
        #[cfg(feature = "managed-p2p")]
        if let Some(p2p) = &self.p2p {
            p2p.stop_transport().await;
        }

        // No request and no peer asks for an access decision any more, and
        // the redrive has stopped: the Vera node stops while the network
        // runtime still times the wait.
        // A failed stop is returned once the rest of the teardown has run: the
        // Vera node may still hold its directory.
        #[cfg(feature = "embedded-vera")]
        let vera_stopped = match &self.embedded_vera {
            Some(vera) => vera.stop().await,
            None => Ok(()),
        };
        #[cfg(feature = "embedded-vera")]
        match &vera_stopped {
            Ok(()) => tracing::info!("embedded Vera node stopped"),
            Err(error) => tracing::warn!(%error, "embedded Vera node did not stop cleanly"),
        }

        // Stop accepting the rest of the node's network work and drain what is
        // in flight.
        if let Some(network) = &self.network {
            network.drain().await;
        }

        // Nothing opens a transaction for the node any more: those left open
        // (a client gone mid-transaction, a commit refused above) end while
        // their owners' workers still run, so the store's close does not
        // wait out its timeout for them.
        match self
            .txn_registry
            .cleanup_stale_transactions(Duration::ZERO)
            .await
        {
            Ok(ended) if ended.cleaned > 0 => {
                tracing::info!(
                    ended = ended.cleaned,
                    "shutdown ended transactions left open"
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "shutdown could not end the transactions left open");
            }
        }

        // Nothing reaches the data path any more: the worker pool finishes
        // what it admitted and stops, then the store closes.
        let closed = self.database.close().await;

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
        closed.map_err(Error::from)?;
        #[cfg(feature = "embedded-vera")]
        vera_stopped?;
        Ok(())
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

/// The error of a transaction step whose request task never finished.
fn refused_transaction(error: edge::EdgeError, step: &str) -> query::TransactionError {
    query::TransactionError::Execution(match error {
        edge::EdgeError::Closed => {
            format!("the node is shutting down; the transaction was not {step}")
        }
        error => format!("transaction task failed: {error}"),
    })
}

/// How a request's task is admitted to the node's request scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Admit {
    /// A request of its own: refused once shutdown stops admitting.
    Request,
    /// Work of a request already admitted, such as a backup import's writes
    /// ([`Node::import_backup`]): still
    /// admitted while that request is in flight, so shutdown drains it.
    Continuation,
}

impl Admit {
    fn spawn<F>(self, edge: &edge::Edge, work: F) -> edge::Task<F::Output>
    where
        F: std::future::Future + defra_core::thread_bounds::MaybeSend + 'static,
        F::Output: defra_core::thread_bounds::MaybeSend + 'static,
    {
        match self {
            Self::Request => edge.spawn(work),
            Self::Continuation => edge.spawn_continuation(work),
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

fn timeout_secs(timeout: Duration) -> u64 {
    if timeout.is_zero() {
        0
    } else {
        timeout
            .as_secs()
            .saturating_add(u64::from(timeout.subsec_nanos() > 0))
    }
}

#[cfg(feature = "http")]
fn timeout_millis(timeout: Duration) -> u64 {
    timeout.as_millis().try_into().unwrap_or(u64::MAX)
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
                let client = net::http::Client::new(edge::testing::edge());
                let schema_url = format!("http://{address}/api/v0/collections");
                let created = edge::testing::edge()
                    .timeout(std::time::Duration::from_secs(5), async {
                        loop {
                            if let Ok(response) = client
                                .post(&schema_url)
                                .header(net::http::header::CONTENT_TYPE, "text/plain")
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
                    net::http::StatusCode::SERVICE_UNAVAILABLE,
                    "embedded collection observation must not enable destructive management"
                );

                node.shutdown().await.unwrap();
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
                )
                .expect("the store takes the identity");

                let node = Node::builder()
                    .with_node_identity_did(did)
                    .build()
                    .await
                    .expect("registered remote signer should build");

                assert_eq!(node.node_identity_did(), Some(did));
                node.shutdown().await.unwrap();
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

#[cfg(all(test, feature = "p2p"))]
mod p2p_line_tests;

#[cfg(test)]
mod owner_observation_tests;

#[cfg(test)]
#[path = "../tests/unit/delegated_nac.rs"]
mod delegated_nac_tests;
#[cfg(test)]
#[path = "../tests/unit/source_nac.rs"]
mod source_nac_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../tests/unit/document_projector.rs"]
mod document_projector_tests;
#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../tests/unit/projection_access_notification.rs"]
mod projection_access_notification_tests;
#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../tests/unit/related_projection.rs"]
mod related_projection_tests;
#[cfg(test)]
#[path = "../tests/unit/userset_manager.rs"]
mod userset_manager_tests;

#[cfg(test)]
#[path = "../tests/unit/transactional_relationship.rs"]
mod transactional_relationship_tests;

#[cfg(test)]
#[path = "../tests/unit/dac_bypass_denied.rs"]
mod dac_bypass_denied_tests;

#[cfg(test)]
#[path = "../tests/unit/administrative_relationship.rs"]
mod administrative_relationship_tests;
#[cfg(test)]
#[path = "../tests/unit/delegated_relationship.rs"]
mod delegated_relationship_tests;

#[cfg(all(test, feature = "vera", not(target_arch = "wasm32")))]
#[path = "../tests/unit/staged_relationship.rs"]
mod staged_relationship_tests;

#[cfg(all(test, feature = "vera", not(target_arch = "wasm32")))]
#[path = "../tests/unit/remote_owner_observation.rs"]
mod remote_owner_observation_tests;

#[cfg(all(test, feature = "vera", not(target_arch = "wasm32")))]
#[path = "../tests/unit/strict_access_reads.rs"]
mod strict_access_reads_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../tests/unit/verified_create.rs"]
mod verified_create_tests;

#[cfg(all(test, feature = "vera", not(target_arch = "wasm32")))]
#[path = "../tests/unit/verified_remote.rs"]
mod verified_remote_tests;

#[cfg(all(test, feature = "vera", not(target_arch = "wasm32")))]
#[path = "../tests/unit/verified_edit.rs"]
mod verified_edit_tests;

#[cfg(all(test, feature = "vera", not(target_arch = "wasm32")))]
#[path = "../tests/unit/owed_report.rs"]
mod owed_report_tests;

#[cfg(all(test, feature = "http"))]
mod http_signing_tests;

#[cfg(all(test, feature = "http"))]
#[path = "../tests/unit/http_acp.rs"]
mod http_acp_tests;

#[cfg(all(test, feature = "http", feature = "vera", not(target_arch = "wasm32")))]
#[path = "../tests/unit/http_staged_relationship.rs"]
mod http_staged_relationship_tests;

#[cfg(all(test, feature = "http"))]
mod router_transform_tests;

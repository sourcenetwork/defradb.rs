//! The node crate's error type.
//!
//! One enum for the embedded node, its trait surfaces and the benchmark
//! helpers. Each variant names one distinct failure and carries its source
//! (or the offending name), so a message reads the same wherever it is raised.

/// Failure of an embedded node operation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The database rejected the operation.
    #[error(transparent)]
    Db(#[from] db::Error),
    /// A DID or identity key was rejected.
    #[error(transparent)]
    Identity(#[from] identity::Error),
    /// An HTTP request to an embedding service failed.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// A file or directory operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A JSON document could not be read or written.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// The P2P transport failed.
    #[cfg(feature = "p2p")]
    #[error(transparent)]
    P2p(#[from] p2p::Error),
    /// The HTTP server failed.
    #[cfg(feature = "http")]
    #[error(transparent)]
    HttpServer(#[from] defra_http::HttpError),
    /// A transaction operation failed.
    #[error(transparent)]
    Transaction(#[from] query::TransactionError),

    /// An access-control check or policy operation failed.
    #[error(transparent)]
    Acp(#[from] acp::Error),
    /// The node network runtime did not start.
    #[error(transparent)]
    EdgeRuntime(#[from] edge::EdgeError),
    /// The policy YAML was rejected.
    #[error("{0}")]
    PolicyYaml(String),
    /// A signed block could not be read or verified.
    #[error("{0}")]
    BlockAccess(String),

    /// A schema policy does not match its policy definition.
    #[error("schema policy validation error: {0}")]
    PolicyValidation(String),
    /// The identity acting on a request is malformed.
    #[error("malformed acting identity: {0}")]
    InvalidActingIdentity(#[source] identity::Error),
    /// The schema SDL did not parse.
    #[error("SDL parse error: {0}")]
    SchemaParse(#[source] query::QueryError),
    /// The parsed schema is invalid.
    #[error("schema validation error: {0}")]
    SchemaValidation(String),
    /// Creating the collections of a schema failed.
    #[error("create collection error: {0}")]
    CollectionCreate(#[source] db::Error),
    /// The target SDL of a view did not parse.
    #[error("view SDL parse error: {0}")]
    ViewSchemaParse(#[source] query::QueryError),
    /// The source query of a view did not parse.
    #[error("view query parse error: {0}")]
    ViewQueryParse(#[source] query::QueryError),
    /// The source query of a view selects nothing.
    #[error("invalid view query: no selections found")]
    ViewQueryEmpty,
    /// A downsample definition is invalid.
    #[error("invalid downsample definition: {0}")]
    InvalidDownsample(#[source] db::Error),
    /// Creating the collection behind a view failed.
    #[error("create view collection error: {0}")]
    ViewCollectionCreate(#[source] db::Error),
    /// Refreshing materialized views failed.
    #[error("refresh materialized views error: {0}")]
    MaterializedViewRefresh(#[source] db::Error),
    /// Bootstrapping downsample collections failed.
    #[error("bootstrap downsample collections error: {0}")]
    DownsampleBootstrap(#[source] db::Error),

    /// Looking a policy up failed.
    #[error("failed to get policy: {0}")]
    PolicyLookup(#[source] zanzibar::error::Error),
    /// Looking a policy up on the on-chain provider failed.
    #[cfg(feature = "vera")]
    #[error("failed to get policy: {0}")]
    VeraPolicyLookup(#[source] vera::ProviderError),
    /// The operation needs ACP and none is configured.
    #[error("operation requires ACP, but ACP not available")]
    AcpUnavailable,
    /// An identity string is not a DID.
    #[error("invalid identity DID: {0}")]
    InvalidIdentityDid(#[source] identity::Error),
    /// Looking a collection up failed.
    #[error("failed to get collection '{collection}': {source}")]
    CollectionLookup {
        /// The collection that was looked up.
        collection: String,
        /// Why the lookup failed.
        source: db::Error,
    },
    /// The named collection does not exist.
    #[error("collection '{0}' does not exist")]
    CollectionMissing(String),
    /// The collection has no policy to manage relationships on.
    #[error("operation requires ACP, but collection has no policy")]
    CollectionWithoutPolicy,
    /// A relationship target did not parse.
    #[error("invalid target: {0}")]
    InvalidTarget(#[source] acp::Error),
    /// The named policy does not exist.
    #[error("policy '{0}' not found")]
    PolicyNotFound(String),
    /// The policy has no such resource.
    #[error("resource '{0}' not found in policy")]
    ResourceNotFound(String),
    /// The policy resource has no such relation.
    #[error("relation '{relation}' not found in policy resource '{resource}'")]
    RelationNotFound {
        /// The missing relation.
        relation: String,
        /// The resource that was searched.
        resource: String,
    },
    /// A policy was submitted without a creator.
    #[error("policy creator can not be empty")]
    PolicyCreatorEmpty,
    /// A policy was submitted without data.
    #[error("policy data can not be empty")]
    PolicyDataEmpty,
    /// A policy has no name.
    #[error("name required")]
    PolicyNameRequired,
    /// An explicit policy counter was given to a remote ACP.
    #[error("explicit policy counters apply to local ACP only")]
    ExplicitCounterRemote,
    /// The on-chain provider rejected the policy.
    #[cfg(feature = "vera")]
    #[error("Vera create policy failed: {0}")]
    VeraCreatePolicy(#[source] vera::ProviderError),
    /// Allocating a policy ID failed.
    #[error("failed to allocate policy ID: {0}")]
    PolicyIdAllocation(#[source] zanzibar::error::Error),
    /// The policy is invalid.
    #[error("invalid policy: {0}")]
    InvalidPolicy(#[source] acp::Error),
    /// Storing the policy failed.
    #[error("failed to store policy: {0}")]
    PolicyStore(#[source] zanzibar::error::Error),
    /// Owner relations are managed by the system only.
    #[error("OPERATION_FORBIDDEN: cannot add owner relation")]
    OwnerRelationAdd,
    /// Owner relations are managed by the system only.
    #[error("OPERATION_FORBIDDEN: cannot delete owner relation")]
    OwnerRelationDelete,
    /// The on-chain provider could not be created.
    #[cfg(feature = "vera")]
    #[error("failed to create Vera provider: {0}")]
    VeraProvider(#[source] vera::ProviderError),

    /// A call did not run on the worker pool.
    #[error("the call did not run on the worker pool: {0}")]
    WorkerPoolCall(#[source] workers::Error),
    /// The worker pool did not start.
    #[error("the worker pool did not start: {0}")]
    WorkerPoolStart(#[source] workers::Error),
    /// A peer operation needs P2P and the node has none.
    #[error("P2P is not enabled for this node")]
    P2pDisabled,
    /// A peer ID did not parse.
    #[cfg(feature = "p2p")]
    #[error("invalid peer ID: {0}")]
    InvalidPeerId(#[source] defra_http::P2PError),
    /// Authorizing a peer failed.
    #[cfg(feature = "p2p")]
    #[error("failed to authorize peer: {0}")]
    PeerAuthorize(#[source] defra_http::P2PError),
    /// Revoking a peer failed.
    #[cfg(feature = "p2p")]
    #[error("failed to revoke peer: {0}")]
    PeerRevoke(#[source] defra_http::P2PError),
    /// The Iroh peer did not start.
    #[cfg(feature = "p2p")]
    #[error("iroh peer failed to start: {0}")]
    IrohStart(#[source] p2p::Error),
    /// The HTTP server was configured on a node with no network runtime.
    #[error("the HTTP server needs the network runtime")]
    HttpNeedsNetwork,
    /// P2P was configured on a node with no network runtime.
    #[error("P2P needs the network runtime")]
    P2pNeedsNetwork,
    /// The node identity DID is not in the signing registry.
    #[error("node identity DID {0} is not registered in the DefraDB signing registry")]
    NodeIdentityUnregistered(String),
    /// The registered node identity has no way to sign.
    #[error("node identity DID {0} is registered without local key bytes or a remote signer")]
    NodeIdentityUnusable(String),
    /// The registered node identity key did not load.
    #[error("failed to load registered node identity {did}: {source}")]
    NodeIdentityLoad {
        /// The registered DID.
        did: String,
        /// Why the key did not load.
        source: identity::Error,
    },
    /// The DID of a registered node identity could not be derived.
    #[error("failed to derive registered node identity DID: {0}")]
    NodeIdentityDerive(#[source] identity::Error),
    /// The key derives a different DID than the one registered.
    #[error("registered node identity DID mismatch: expected {expected}, derived {derived}")]
    NodeIdentityMismatch {
        /// The registered DID.
        expected: String,
        /// The DID the key derives.
        derived: String,
    },
    /// The registered key type is not supported.
    #[error("unsupported registered node identity key type {0}")]
    UnsupportedKeyType(String),
    /// The node identity DID given to the builder is invalid.
    #[error("invalid node identity DID: {0}")]
    InvalidNodeIdentityDid(#[source] identity::Error),
    /// Node ACP needs a node identity.
    #[error("node ACP requires a node identity; set one with with_node_identity_did()")]
    NodeAcpNeedsIdentity,
    /// The node identity given for node ACP is invalid.
    #[error("invalid node identity DID for node ACP: {0}")]
    InvalidNodeAcpIdentity(#[source] identity::Error),
    /// Node ACP needs a locally held key.
    #[error(
        "node ACP requires the node identity to be backed by local key bytes so the node DID can own the policy"
    )]
    NodeAcpNeedsLocalKey,
    /// Enabling node ACP failed.
    #[error("failed to enable node ACP: {0}")]
    NodeAcpEnable(#[source] db::nac::Error),
    /// The at-rest encryption keys were rejected.
    #[error("invalid at-rest encryption keys: {0}")]
    InvalidAtRestKeys(#[source] storage::Error),
    /// The regolith store did not open.
    #[error("failed to open regolith store: {0}")]
    StoreOpen(#[source] storage::Error),
    /// The in-memory regolith store did not open.
    #[error("failed to open in-memory regolith: {0}")]
    MemoryStoreOpen(#[source] storage::Error),
    /// The database did not open.
    #[error("failed to open database: {0}")]
    DatabaseOpen(#[source] db::Error),
    /// The transaction cleanup interval is zero while idle cleanup is on.
    #[error("transaction cleanup interval must be non-zero when idle cleanup is enabled")]
    ZeroCleanupInterval,
    /// A response lacks a field of a record.
    #[error("missing {field} in {within}")]
    MissingIn {
        /// The missing field.
        field: &'static str,
        /// The record that lacks it.
        within: String,
    },
    /// A response lacks a field named at run time.
    #[error("missing {field} in {within}")]
    MissingField {
        /// The missing field.
        field: String,
        /// The record that lacks it.
        within: String,
    },
    /// A response lacks a list of records.
    #[error("missing {field} array for {owner}")]
    MissingArrayFor {
        /// The missing list.
        field: &'static str,
        /// The record that should own it.
        owner: String,
    },
    /// A response lacks the list of a collection.
    #[error("missing {collection} array in {within}")]
    MissingArray {
        /// The collection whose list is missing.
        collection: String,
        /// The response that lacks it.
        within: String,
    },
    /// A response lacks the document ID of a session.
    #[error("missing session _docID for {0}")]
    MissingSessionDocId(String),
    /// A response lacks the data of a query.
    #[error("missing response data for {0}")]
    MissingResponseData(String),
    /// A response lacks the document ID of a created record.
    #[error("missing {kind} for {owner} response={response}")]
    MissingCreatedDocId {
        /// What is missing, naming the record kind.
        kind: &'static str,
        /// What the record was created for.
        owner: String,
        /// The response received.
        response: String,
    },
    /// No session carries the given ID.
    #[error("no CodingSession found for session_id={0}")]
    SessionNotFound(String),
    /// A query came back with errors.
    #[error("{context}: {messages}")]
    QueryFailed {
        /// What the query was doing.
        context: String,
        /// The errors it returned.
        messages: String,
    },
    /// No retrieval task could be built from the fixture.
    #[error("failed to build any context-1-style coding retrieval tasks")]
    NoRetrievalTasks,
    /// A benchmark was asked for zero iterations.
    #[error("iterations must be greater than zero")]
    ZeroIterations,
}

impl Error {
    /// A response lacks `field` in the record rendered by `within`.
    pub fn missing(field: &'static str, within: impl std::fmt::Display) -> Self {
        Self::MissingIn {
            field,
            within: within.to_string(),
        }
    }
}

/// Result alias for node operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

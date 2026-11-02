//! The node crate's error type.
//!
//! One enum for the node, its trait surfaces and the benchmark
//! helpers. Each variant names one distinct failure and carries its source
//! (or the offending name), so a message reads the same wherever it is raised.

/// Failure of a node operation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The fragment nesting limit does not fit a query thread's stack.
    #[error(transparent)]
    FragmentNesting(#[from] query::FragmentNestingError),
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
    /// The transaction does not exist, is finished, or belongs to another
    /// caller. Go's text for all three.
    #[error("transaction not found")]
    TransactionNotFound,
    /// The commit lost a write conflict. Go's text.
    #[error("transaction conflict. Please retry")]
    TransactionConflict,
    /// A custom CRDT type could not be registered.
    #[error(transparent)]
    CustomCrdt(#[from] crdt::RegistryError),

    /// An access-control check or policy operation failed.
    #[error(transparent)]
    Acp(#[from] acp::Error),
    /// The node network runtime did not start.
    #[error(transparent)]
    EdgeRuntime(#[from] edge::EdgeError),
    /// A DAC policy operation was refused.
    #[error(transparent)]
    DacPolicy(#[from] db::DacPolicyError),
    /// A signed block could not be read or verified.
    #[error("{0}")]
    BlockAccess(String),

    /// A schema policy does not match its policy definition.
    #[error("schema policy validation error: {0}")]
    PolicyValidation(String),
    /// A lens or the lens listing is not the JSON Go writes; serde's message.
    #[error("{0}")]
    LensJson(#[source] serde_json::Error),
    /// A peer operation was refused.
    #[cfg(feature = "managed-p2p")]
    #[error(transparent)]
    Peer(#[from] defra_p2p_adapter::peer::Error),
    /// A document write was refused.
    #[error(transparent)]
    Documents(#[from] query::documents::Error),
    /// An add-collection request was refused.
    #[error(transparent)]
    AddCollection(#[from] db::AddCollectionError),
    /// The identity acting on a request is malformed.
    #[error("malformed acting identity: {0}")]
    InvalidActingIdentity(#[source] identity::Error),
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
    #[cfg(feature = "managed-p2p")]
    #[error("invalid peer ID: {0}")]
    InvalidPeerId(#[source] defra_p2p_adapter::P2PError),
    /// Authorizing a peer failed.
    #[cfg(feature = "managed-p2p")]
    #[error("failed to authorize peer: {0}")]
    PeerAuthorize(#[source] defra_p2p_adapter::P2PError),
    /// Revoking a peer failed.
    #[cfg(feature = "managed-p2p")]
    #[error("failed to revoke peer: {0}")]
    PeerRevoke(#[source] defra_p2p_adapter::P2PError),
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

    /// The node's store failed.
    #[error("{0}")]
    Storage(#[from] storage::corekv::Error),
    /// Access hooks were given to a node whose transport is not iroh.
    #[cfg(feature = "p2p")]
    #[error("access hooks require the iroh transport")]
    AccessHooksNeedIroh,
    /// The P2P assembly did not finish on the network runtime.
    #[cfg(feature = "libp2p")]
    #[error("P2P setup did not finish on the network runtime: {0}")]
    NetworkSetup(#[source] edge::EdgeError),
    /// NAC could not be initialised.
    #[error("failed to initialize NAC: {0}")]
    NacInit(#[source] db::nac::Error),
    /// The replicator retry ladder is not valid.
    #[error("invalid replicator retry intervals: {0}")]
    InvalidRetrySchedule(String),
    /// A node signing key did not load.
    #[error("failed to load {curve} key: {source}")]
    KeyLoad {
        /// The key's curve.
        curve: &'static str,
        /// Why it did not load.
        #[source]
        source: crypto::Error,
    },
    /// A node signing key could not be generated.
    #[error("failed to generate node signing key: {0}")]
    KeyGenerate(#[source] crypto::Error),
    /// A node identity could not be created from its key.
    #[error("failed to create node identity: {0}")]
    IdentityCreate(#[source] identity::Error),
    /// The DID of a node identity could not be derived.
    #[error("failed to derive node DID: {0}")]
    DidDerive(#[source] identity::Error),
    /// A libp2p listen address did not parse.
    #[cfg(feature = "libp2p")]
    #[error("invalid multiaddr '{addr}': {source}")]
    InvalidMultiaddr {
        /// The address.
        addr: String,
        /// Why it did not parse.
        #[source]
        source: libp2p::multiaddr::Error,
    },
    /// The peer key is not a libp2p keypair.
    #[cfg(feature = "libp2p")]
    #[error("invalid peer key: {0}")]
    InvalidPeerKey(#[source] libp2p::identity::DecodingError),
    /// The libp2p host did not start.
    #[cfg(feature = "libp2p")]
    #[error("failed to create P2P host: {0}")]
    HostCreate(#[source] p2p::Error),
    /// The libp2p host did not listen.
    #[cfg(feature = "libp2p")]
    #[error("failed to start listening: {0}")]
    Listen(#[source] p2p::Error),
    /// The sync coordinator did not start.
    #[cfg(feature = "libp2p")]
    #[error("failed to create sync coordinator: {0}")]
    SyncCoordinator(#[source] p2p::Error),
    /// The KMS transport did not start.
    #[cfg(feature = "libp2p")]
    #[error("failed to create KMS transport: {0}")]
    KmsTransport(#[source] kms::Error),
    /// The store holds a plaintext peer key, which a keyring cannot take over.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("plaintext peerstore /p2p/local-peer-key requires explicit keyring migration")]
    PlaintextPeerKey,
    /// A plaintext iroh key file exists, which a keyring cannot take over.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("plaintext iroh key file '{}' requires explicit keyring migration", .0.display())]
    PlaintextIrohKeyFile(std::path::PathBuf),
    /// A plaintext legacy key exists, which a keyring cannot take over.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("plaintext peerstore {0} requires explicit keyring migration")]
    PlaintextLegacyKey(&'static str),
    /// The keyring's peer key is not valid.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("keyring peer key is corrupt: {0}")]
    KeyringKeyCorrupt(#[source] crypto::Error),
    /// The database names a keyring key that the keyring no longer holds.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("peer key is missing from the configured keyring; restore the original keyring")]
    KeyringKeyMissing,
    /// A peer key could not be generated.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("failed to generate peer key: {0}")]
    PeerKeyGenerate(#[source] crypto::Error),
    /// The keyring did not take the peer key.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("failed to persist peer key in keyring: {0}")]
    KeyringPersist(#[source] keyring::Error),
    /// The keyring could not be read.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("failed to read peer key from keyring: {0}")]
    KeyringRead(#[source] keyring::Error),
    /// The keyring's peer identity is not the one this database recorded.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("keyring peer identity does not match this database")]
    KeyringIdentityMismatch,
    /// The database was started with a keyring and needs it again.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("this database requires its application-owned peer keyring")]
    PeerKeyringRequired,
    /// The stored peer key could not be read.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("failed to read peer key: {0}")]
    PeerKeyRead(#[source] storage::corekv::Error),
    /// The peer key could not be stored.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("failed to store peer key: {0}")]
    PeerKeyStore(#[source] storage::corekv::Error),
    /// The stored peer key has the wrong length.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("stored peer key is corrupt: {0}")]
    StoredPeerKeyCorrupt(#[source] SeedLength),
    /// An iroh key file has the wrong length.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("iroh key file '{}' is corrupt: {source}", path.display())]
    IrohKeyFileCorrupt {
        /// The file.
        path: std::path::PathBuf,
        /// Why it is corrupt.
        #[source]
        source: SeedLength,
    },
    /// An iroh key file could not be read.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("failed to read iroh key file '{}': {source}", path.display())]
    IrohKeyFileRead {
        /// The file.
        path: std::path::PathBuf,
        /// Why it could not be read.
        #[source]
        source: std::io::Error,
    },
    /// The legacy libp2p peer key could not be read.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error("failed to read legacy libp2p peer key: {0}")]
    LegacyKeyRead(#[source] storage::corekv::Error),
    /// The legacy libp2p peer key is not a key.
    #[cfg(feature = "libp2p")]
    #[error("legacy libp2p peer key is corrupt: {0}")]
    LegacyKeyCorrupt(#[source] libp2p::identity::DecodingError),
    /// The legacy libp2p peer key is not Ed25519.
    #[cfg(feature = "libp2p")]
    #[error("legacy libp2p peer key is not Ed25519")]
    LegacyKeyNotEd25519,
    /// The legacy libp2p peer key could not be removed.
    #[cfg(feature = "libp2p")]
    #[error("failed to remove legacy libp2p peer key: {0}")]
    LegacyKeyRemove(#[source] storage::corekv::Error),
    /// A peer key seed has the wrong length.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    #[error(transparent)]
    SeedLength(#[from] SeedLength),
    /// The encrypted index names a field the collection lacks.
    #[error("encrypted index on non-existent field: {0}")]
    EncryptedIndexFieldMissing(String),
    /// The field already has an encrypted index.
    #[error("encrypted index already exists on field: {0}")]
    EncryptedIndexExists(String),
    /// The collection's schema did not serialize.
    #[error("failed to serialize schema: {0}")]
    SchemaSerialize(#[source] serde_json::Error),
    /// The systemstore did not open.
    #[error("failed to get systemstore: {0}")]
    SystemstoreOpen(#[source] db::Error),
    /// The schema did not save.
    #[error("failed to save schema: {0}")]
    SchemaSave(#[source] storage::corekv::Error),
    /// The collection name mapping did not save.
    #[error("failed to save name mapping: {0}")]
    NameMappingSave(#[source] storage::corekv::Error),
    /// The commit failed.
    #[error("failed to commit: {0}")]
    Commit(#[source] db::Error),
    /// The collection cache did not reload.
    #[error("failed to reload cache: {0}")]
    CacheReload(#[source] db::Error),
    /// A transaction could not be created.
    #[error("failed to create transaction: {0}")]
    TxnCreate(#[source] db::Error),
    /// A collection lookup failed.
    #[error("get collection error: {0}")]
    CollectionLookupFailed(#[source] db::Error),
}

/// A peer key seed was not 32 bytes.
#[cfg(any(feature = "libp2p", feature = "p2p"))]
#[derive(Debug, thiserror::Error)]
#[error("expected a 32-byte Ed25519 seed, got {0} bytes")]
pub struct SeedLength(pub usize);

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

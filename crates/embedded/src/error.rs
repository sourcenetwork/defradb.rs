//! The embedded crate's error type: every variant names what failed and carries
//! the underlying error, so a caller can match on the step and read the cause.

use std::path::PathBuf;

/// Why an embedded node could not be built or an operation on it failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The node's store failed.
    #[error("{0}")]
    Storage(#[from] storage::corekv::Error),

    #[error("the worker pool did not start: {0}")]
    PoolStart(#[source] workers::Error),
    #[error("the call did not run on the worker pool: {0}")]
    PoolRun(#[source] workers::Error),
    #[error("invalid at-rest encryption keys: {0}")]
    AtRestKeys(#[source] storage::corekv::Error),
    #[error("failed to create parent directory for '{}': {source}", path.display())]
    DataDirCreate {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to open regolith store at '{}': {source}", path.display())]
    StoreOpen {
        path: PathBuf,
        #[source]
        source: storage::corekv::Error,
    },
    #[error("failed to open in-memory regolith store: {0}")]
    MemoryStoreOpen(#[source] storage::corekv::Error),
    #[error("failed to open database: {0}")]
    DbOpen(#[source] db::Error),
    #[error("access hooks require the iroh transport")]
    AccessHooksNeedIroh,
    #[error("the network runtime did not start: {0}")]
    NetworkStart(#[source] edge::EdgeError),
    #[error("P2P setup did not finish on the network runtime: {0}")]
    NetworkSetup(#[source] edge::EdgeError),

    #[error("SDL parse error: {0}")]
    SchemaParse(#[source] query::QueryError),
    #[error("schema validation error: {0}")]
    SchemaValidation(String),
    #[error("create collection error: {0}")]
    CollectionCreate(#[source] db::Error),
    #[error("get collection error: {0}")]
    CollectionLookup(#[source] db::Error),
    #[error("collection '{0}' not found")]
    CollectionNotFound(String),
    #[error("encrypted index on non-existent field: {0}")]
    EncryptedIndexFieldMissing(String),
    #[error("encrypted index already exists on field: {0}")]
    EncryptedIndexExists(String),
    #[error("failed to create transaction: {0}")]
    TxnCreate(#[source] db::Error),
    #[error("failed to serialize schema: {0}")]
    SchemaSerialize(#[source] serde_json::Error),
    #[error("failed to get systemstore: {0}")]
    SystemstoreOpen(#[source] db::Error),
    #[error("failed to save schema: {0}")]
    SchemaSave(#[source] storage::corekv::Error),
    #[error("failed to save name mapping: {0}")]
    NameMappingSave(#[source] storage::corekv::Error),
    #[error("failed to commit: {0}")]
    Commit(#[source] db::Error),
    #[error("failed to reload cache: {0}")]
    CacheReload(#[source] db::Error),

    #[cfg(feature = "vera")]
    #[error("failed to create Vera provider: {0}")]
    VeraProvider(#[source] vera::ProviderError),
    #[error("failed to initialize NAC: {0}")]
    NacInit(#[source] db::nac::Error),
    #[error("node access control at startup needs a node identity to own it")]
    NodeAcpWithoutIdentity,
    #[error("malformed node identity: {0}")]
    InvalidNodeIdentityDid(#[source] identity::Error),
    #[error("invalid replicator retry intervals: {0}")]
    InvalidRetrySchedule(String),

    #[error("malformed acting identity: {0}")]
    InvalidActingIdentity(#[source] identity::Error),
    #[error("failed to load {curve} key: {source}")]
    KeyLoad {
        curve: &'static str,
        #[source]
        source: crypto::Error,
    },
    #[error("failed to generate node signing key: {0}")]
    KeyGenerate(#[source] crypto::Error),
    #[error("failed to create node identity: {0}")]
    IdentityCreate(#[source] identity::Error),
    #[error("failed to derive node DID: {0}")]
    DidDerive(#[source] identity::Error),
    #[error("no signing identity registered for DID: {0}")]
    IdentityNotRegistered(String),
    #[error("registered identity {0} has neither a local key nor a remote signer")]
    IdentityUnusable(String),
    #[error("failed to derive stored identity DID: {0}")]
    StoredDidDerive(#[source] identity::Error),
    #[error("registered identity DID mismatch: expected {expected}, derived {derived}")]
    IdentityDidMismatch { expected: String, derived: String },
    #[error("failed to load stored {curve} key: {source}")]
    StoredKeyLoad {
        curve: &'static str,
        #[source]
        source: crypto::Error,
    },
    #[error("failed to create stored {curve} identity: {source}")]
    StoredIdentityCreate {
        curve: &'static str,
        #[source]
        source: identity::Error,
    },
    #[error("stored identity {0} cannot be used as a node identity")]
    StoredIdentityUnsupported(String),

    #[cfg(feature = "libp2p")]
    #[error("invalid multiaddr '{addr}': {source}")]
    InvalidMultiaddr {
        addr: String,
        #[source]
        source: libp2p::multiaddr::Error,
    },
    #[cfg(feature = "libp2p")]
    #[error("invalid peer key: {0}")]
    InvalidPeerKey(#[source] libp2p::identity::DecodingError),
    #[error("failed to create P2P host: {0}")]
    HostCreate(#[source] p2p::Error),
    #[error("failed to start listening: {0}")]
    Listen(#[source] p2p::Error),
    #[error("failed to create sync coordinator: {0}")]
    SyncCoordinator(#[source] p2p::Error),
    #[error("failed to create KMS transport: {0}")]
    KmsTransport(#[source] kms::Error),
    #[error("failed to start iroh peer: {0}")]
    IrohStart(#[source] p2p::Error),

    #[error("plaintext peerstore /p2p/local-peer-key requires explicit keyring migration")]
    PlaintextPeerKey,
    #[error("plaintext iroh key file '{}' requires explicit keyring migration", .0.display())]
    PlaintextIrohKeyFile(PathBuf),
    #[error("plaintext peerstore {0} requires explicit keyring migration")]
    PlaintextLegacyKey(&'static str),
    #[error("keyring peer key is corrupt: {0}")]
    KeyringKeyCorrupt(#[source] crypto::Error),
    #[error("peer key is missing from the configured keyring; restore the original keyring")]
    KeyringKeyMissing,
    #[error("failed to generate peer key: {0}")]
    PeerKeyGenerate(#[source] crypto::Error),
    #[error("failed to persist peer key in keyring: {0}")]
    KeyringPersist(#[source] keyring::Error),
    #[error("failed to read peer key from keyring: {0}")]
    KeyringRead(#[source] keyring::Error),
    #[error("keyring peer identity does not match this database")]
    KeyringIdentityMismatch,
    #[error("this database requires its application-owned peer keyring")]
    PeerKeyringRequired,
    #[error("failed to read peer key: {0}")]
    PeerKeyRead(#[source] storage::corekv::Error),
    #[error("failed to store peer key: {0}")]
    PeerKeyStore(#[source] storage::corekv::Error),
    #[error("stored peer key is corrupt: {0}")]
    StoredPeerKeyCorrupt(#[source] SeedLength),
    #[error("iroh key file '{}' is corrupt: {source}", path.display())]
    IrohKeyFileCorrupt {
        path: PathBuf,
        #[source]
        source: SeedLength,
    },
    #[error("failed to read iroh key file '{}': {source}", path.display())]
    IrohKeyFileRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read legacy libp2p peer key: {0}")]
    LegacyKeyRead(#[source] storage::corekv::Error),
    #[cfg(feature = "libp2p")]
    #[error("legacy libp2p peer key is corrupt: {0}")]
    LegacyKeyCorrupt(#[source] libp2p::identity::DecodingError),
    #[cfg(feature = "libp2p")]
    #[error("legacy libp2p peer key is not Ed25519")]
    LegacyKeyNotEd25519,
    #[cfg(feature = "libp2p")]
    #[error("failed to remove legacy libp2p peer key: {0}")]
    LegacyKeyRemove(#[source] storage::corekv::Error),
    #[error(transparent)]
    SeedLength(#[from] SeedLength),
}

/// A peer key seed was not 32 bytes.
#[derive(Debug, thiserror::Error)]
#[error("expected a 32-byte Ed25519 seed, got {0} bytes")]
pub struct SeedLength(pub usize);

/// The result of an embedded node operation.
pub type Result<T, E = Error> = std::result::Result<T, E>;

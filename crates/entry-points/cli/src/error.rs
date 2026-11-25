//! CLI error types

use std::path::PathBuf;

use thiserror::Error;

/// CLI result type
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// The commit outcome the server answered with, or that the client
    /// concluded for a write it got no answer to (`defra.txn.commit_outcome`).
    pub fn commit_outcome(&self) -> Option<defra_core::CommitOutcome> {
        match self {
            Self::Reply { outcome, .. } => *outcome,
            Self::Unanswered(_) => Some(defra_core::CommitOutcome::Indeterminate),
            _ => None,
        }
    }

    /// This server answer to a write, `indeterminate` unless the server said
    /// what the commit did: the write may have committed before the server
    /// failed (`defra.txn.commit_outcome.indeterminate`).
    pub fn unless_outcome_known(self) -> Self {
        let indeterminate = Some(defra_core::CommitOutcome::Indeterminate);
        match self {
            Self::Server(message) => Self::Reply {
                message,
                code: None,
                outcome: indeterminate,
            },
            Self::Reply {
                message,
                code,
                outcome: None,
            } => Self::Reply {
                message,
                code,
                outcome: indeterminate,
            },
            other => other,
        }
    }

    /// The wire code the server answered with, such as `STORAGE_FULL`.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Reply { code, .. } => code.as_deref(),
            _ => None,
        }
    }
}

/// CLI errors
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("failed to determine home directory")]
    HomeDirectory,

    #[error("failed to create directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to read config file {path}: {source}")]
    ReadConfig {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse config file {path}: {source}")]
    ParseConfig {
        path: PathBuf,
        source: serde_yaml::Error,
    },

    #[error("failed to read secret file {path}: {source}")]
    ReadSecretFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid secret file {path} at line {line}")]
    ParseSecretFile { path: PathBuf, line: usize },

    #[error("failed to write config file {path}: {source}")]
    WriteConfig {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to serialize config: {0}")]
    SerializeConfig(#[from] serde_yaml::Error),

    #[error("invalid log level: {0}")]
    InvalidLogLevel(String),

    #[error("invalid log format: {0}")]
    InvalidLogFormat(String),

    #[error("invalid log output: {0}")]
    InvalidLogOutput(String),

    #[error("invalid keyring backend: {0}")]
    InvalidKeyringBackend(String),

    #[error("invalid datastore type: {0}")]
    InvalidDatastore(String),

    #[error("invalid transport type: {0}")]
    InvalidTransport(String),

    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("invalid ACP type: {0}")]
    InvalidAcpType(String),

    #[error("storage error: {0}")]
    Storage(#[from] storage::Error),

    #[error("P2P error: {0}")]
    P2P(#[from] p2p::Error),

    #[error("crypto error: {0}")]
    Crypto(#[from] crypto::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("logging initialization failed: {0}")]
    LoggingInit(String),

    #[error("signal handling error: {0}")]
    Signal(String),

    #[error("invalid multiaddr: {0}")]
    InvalidMultiaddr(String),

    #[error("JSON serialization error: {0}")]
    JsonSerialization(#[from] serde_json::Error),

    #[error("invalid API address '{0}': {1}")]
    InvalidApiAddress(String, String),

    #[error(
        "incomplete TLS configuration: both pubkey_path and privkey_path must be set, or neither"
    )]
    IncompleteTlsConfig,

    #[error("keyring error: {0}")]
    Keyring(String),

    #[error("identity error: {0}")]
    Identity(#[from] identity::Error),

    #[error("invalid identity: {0}")]
    InvalidIdentity(String),

    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] net::http::Error),

    /// A write that was sent and got no answer (a timeout, a connection
    /// closed mid-reply): it may have committed. The same text as
    /// [`Error::HttpRequest`].
    #[error("HTTP request failed: {0}")]
    Unanswered(net::http::Error),

    #[error("server returned error: {0}")]
    Server(String),

    /// A server error that carried a wire code or a commit outcome, with the
    /// same text as [`Error::Server`].
    #[error("server returned error: {message}")]
    Reply {
        message: String,
        code: Option<String>,
        outcome: Option<defra_core::CommitOutcome>,
    },

    #[error(transparent)]
    Documents(#[from] query::documents::Error),

    #[error("failed to read file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Go's `ErrCreateFile`.
    #[error("failed to create file: {source}. Filepath: {path}")]
    CreateFile {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Go's `ErrOpenFile`.
    #[error("failed to open file: {source}. Filepath: {path}")]
    OpenFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("collection not found: {0}")]
    CollectionNotFound(String),

    #[error("missing required input: {0}")]
    MissingInput(String),

    /// A flag's value did not parse. Go's `NewErrParsingArgument`.
    #[error("failed to parse {name}: {reason}")]
    ParsingArgument { name: &'static str, reason: String },

    #[error("invalid identifier: {0}")]
    InvalidIdentifier(String),

    #[error("invalid vector index config: {0}")]
    InvalidVectorIndexConfig(String),

    #[error("operation not permitted whilst development mode is disabled")]
    OperationRequiresDeveloperMode,

    #[error("failed to initialize HTTP client: {0}")]
    HttpClientInit(String),

    #[error("invalid URL '{0}': {1}")]
    InvalidUrl(String, String),
}

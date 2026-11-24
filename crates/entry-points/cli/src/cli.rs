//! Root CLI definition with global flags

use clap::{Parser, Subcommand};

use crate::commands::{
    ClientArgs, IdentityArgs, KeyringArgs, SdlArgs, ServerDumpArgs, StartCommand, StorageArgs,
    VersionArgs,
};
use crate::config::Config;
use crate::error::Result;

/// Shared `value_parser` for the CLI's `Option<bool>` toggle flags. Accepts
/// the usual shell-boolean idioms (`true`/`false`, `1`/`0`, `yes`/`no`,
/// `on`/`off`, case-insensitive) so env-driven CI scripts using `=1`/`=0`
/// work. Defined once and referenced from every `--no-*` / toggle flag in
/// both this module and the `start` subcommand, so the convention can't
/// drift per flag.
pub(crate) fn bool_value_parser() -> clap::builder::BoolishValueParser {
    clap::builder::BoolishValueParser::new()
}

/// DefraDB Edge Database
///
/// DefraDB is the edge database to power the user-centric future.
/// Start a DefraDB node, interact with a local or remote node, and much more.
#[derive(Parser, Debug)]
#[command(name = "defradb")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Directory for persistent data (default: $HOME/.defradb)
    #[arg(long, global = true, env = "DEFRA_ROOTDIR")]
    pub rootdir: Option<String>,

    /// Log level to use. Options are debug, info, error, fatal
    #[arg(long, global = true, env = "DEFRA_LOG_LEVEL")]
    pub log_level: Option<String>,

    /// Log output path. Options are stderr or stdout
    #[arg(long, global = true, env = "DEFRA_LOG_OUTPUT")]
    pub log_output: Option<String>,

    /// Log format to use. Options are text or json
    #[arg(long, global = true, env = "DEFRA_LOG_FORMAT")]
    pub log_format: Option<String>,

    /// Include stacktrace in error and fatal logs
    #[arg(long, global = true, env = "DEFRA_LOG_STACKTRACE", num_args = 0..=1, require_equals = true, default_missing_value = "true", value_parser = bool_value_parser())]
    pub log_stacktrace: Option<bool>,

    /// Include source location in logs
    #[arg(long, global = true, env = "DEFRA_LOG_SOURCE", num_args = 0..=1, require_equals = true, default_missing_value = "true", value_parser = bool_value_parser())]
    pub log_source: Option<bool>,

    /// Logger config overrides. Format `<name>,<key>=<val>,...;<name>,...`
    #[arg(long, global = true, env = "DEFRA_LOG_OVERRIDES")]
    pub log_overrides: Option<String>,

    /// Disable colored log output
    #[arg(long, global = true, env = "DEFRA_NO_LOG_COLOR", num_args = 0..=1, require_equals = true, default_missing_value = "true", value_parser = bool_value_parser())]
    pub no_log_color: Option<bool>,

    /// URL of HTTP endpoint to listen on or connect to
    #[arg(long, global = true, env = "DEFRA_API_ADDRESS")]
    pub url: Option<String>,

    /// Service name to use when using the system backend
    #[arg(long, global = true, env = "DEFRA_KEYRING_NAMESPACE")]
    pub keyring_namespace: Option<String>,

    /// Keyring backend to use. Options are file or system
    #[arg(long, global = true, env = "DEFRA_KEYRING_BACKEND")]
    pub keyring_backend: Option<String>,

    /// Path to store encrypted keys when using the file backend
    #[arg(long, global = true, env = "DEFRA_KEYRING_PATH")]
    pub keyring_path: Option<String>,

    /// Disable the keyring and generate ephemeral keys
    #[arg(long, global = true, env = "DEFRA_NO_KEYRING", num_args = 0..=1, require_equals = true, default_missing_value = "true", value_parser = bool_value_parser())]
    pub no_keyring: Option<bool>,

    /// vera.rs JSON-RPC endpoint (e.g., "http://localhost:8545")
    #[cfg(feature = "vera")]
    #[arg(long, global = true, env = "DEFRA_VERA_RS_ADDRESS")]
    pub vera_rs_address: Option<String>,

    /// Trusted Vera consensus public key from operator configuration (hex)
    #[cfg(feature = "vera")]
    #[arg(long, global = true, env = "DEFRA_VERA_CONSENSUS_KEY")]
    pub vera_consensus_key: Option<String>,

    /// Identifier of the Vera deployment's genesis block from operator
    /// configuration (hex). Without it a switch of the deployment's active
    /// device stops every verified read.
    #[cfg(feature = "vera")]
    #[arg(long, global = true, env = "DEFRA_VERA_GENESIS_ID")]
    pub vera_genesis_id: Option<String>,

    /// Vera deployment identifier from operator configuration
    #[cfg(feature = "vera")]
    #[arg(long, global = true, env = "DEFRA_VERA_DEPLOYMENT_ID")]
    pub vera_deployment_id: Option<u64>,

    /// Operator key of the embedded Vera deployment; repeat for several.
    /// Hex compressed secp256k1 keys, `did:key` for Ed25519 and P-256.
    /// Default: the node identity alone
    #[cfg(feature = "embedded-vera")]
    #[arg(
        long = "vera-embedded-operator",
        global = true,
        env = "DEFRA_VERA_EMBEDDED_OPERATORS",
        value_delimiter = ','
    )]
    pub vera_embedded_operators: Vec<String>,

    /// Operator approvals an administrative change of the embedded Vera
    /// deployment needs (default: 1)
    #[cfg(feature = "embedded-vera")]
    #[arg(long, global = true, env = "DEFRA_VERA_EMBEDDED_THRESHOLD")]
    pub vera_embedded_threshold: Option<u16>,

    /// Consensus listener of the embedded Vera node (default: 127.0.0.1:0)
    #[cfg(feature = "embedded-vera")]
    #[arg(long, global = true, env = "DEFRA_VERA_EMBEDDED_LISTEN")]
    pub vera_embedded_listen: Option<String>,

    /// Path to the file containing secrets. Relative paths use the working directory.
    #[arg(long, global = true, env = "DEFRA_SECRET_FILE")]
    pub secret_file: Option<String>,

    /// Disable OpenTelemetry exporters (no-op unless the binary was built with `--features otel`).
    #[arg(long, global = true, env = "DEFRA_NO_TELEMETRY", num_args = 0..=1, require_equals = true, default_missing_value = "true", value_parser = bool_value_parser())]
    pub no_telemetry: Option<bool>,

    /// Enables development mode features
    #[arg(long, global = true, env = "DEFRA_DEVELOPMENT", num_args = 0..=1, require_equals = true, default_missing_value = "true", value_parser = bool_value_parser())]
    pub development: Option<bool>,

    /// Enable Node Access Control (NAC).
    ///
    /// When enabled, node operations require authentication and authorization
    /// based on the node's identity from the keyring.
    #[arg(long = "node-acp-enable", global = true, env = "DEFRA_ACP_NODE_ENABLE", num_args = 0..=1, require_equals = true, default_missing_value = "true", value_parser = bool_value_parser())]
    pub acp_node_enable: Option<bool>,

    /// Document ACP type. Options are none, local, vera (or remote), or verars
    #[cfg(feature = "vera")]
    #[arg(
        long = "document-acp-type",
        global = true,
        env = "DEFRA_ACP_DOCUMENT_TYPE"
    )]
    pub acp_document_type: Option<crate::config::AcpDocumentType>,

    /// Document ACP type. Options are none or local
    #[cfg(not(feature = "vera"))]
    #[arg(
        long = "document-acp-type",
        global = true,
        env = "DEFRA_ACP_DOCUMENT_TYPE"
    )]
    pub acp_document_type: Option<crate::config::AcpDocumentType>,

    /// Account named as the authorized account in client auth tokens, for
    /// nodes using remote document ACP
    #[arg(
        long = "remote-dac-address",
        global = true,
        env = "DEFRA_ACP_DOCUMENT_REMOTE_ADDRESS"
    )]
    pub remote_dac_address: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

/// Available subcommands
#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
#[non_exhaustive]
pub enum Command {
    /// Start a DefraDB node
    Start(StartCommand),

    /// Display the version information of DefraDB and its components
    Version(VersionArgs),

    /// Manage keys in the keyring
    Keyring(KeyringArgs),

    /// Interact with a DefraDB node
    Client(ClientArgs),

    /// Manage identities
    Identity(IdentityArgs),

    /// Manage SDL (Schema Definition Language)
    Sdl(SdlArgs),

    /// Dump server-side data
    ServerDump(ServerDumpArgs),

    /// Inspect local database storage.
    Storage(StorageArgs),
}

impl Cli {
    /// Execute the CLI command. Network work runs on `edge`.
    pub async fn execute(self, config: Config, edge: edge::Edge) -> Result<()> {
        match self.command {
            Command::Start(args) => args.execute(config, edge).await,
            Command::Version(args) => args.execute(),
            Command::Keyring(args) => args.execute(config),
            Command::Client(args) => args.execute(config, self.url, edge).await,
            Command::Identity(args) => args.execute(config),
            Command::Sdl(args) => args.execute(),
            Command::ServerDump(args) => args.execute(&config).await,
            Command::Storage(args) => args.execute(&config).await,
        }
    }
}

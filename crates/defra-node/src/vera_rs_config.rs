//! Configuration of vera.rs document ACP.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

/// vera.rs document ACP: a Vera node reached over JSON-RPC, with every read
/// verified against an independently provisioned consensus key.
#[derive(Clone)]
pub struct VeraRsConfig {
    /// HTTP JSON-RPC address of the Vera node; headers arrive over the same
    /// address by WebSocket.
    pub rpc_address: String,
    /// Hex consensus key that proofs are verified against. It must come from
    /// the deployment's operators, not from the node being asked.
    pub consensus_key: String,
    /// The deployment the node's worker signs for.
    pub deployment_id: u64,
    /// The 32-byte secp256k1 key of the actor that owns this node's policies.
    pub actor_key: Vec<u8>,
    /// Where the worker journals a submission until Vera confirms it. It must
    /// survive restarts, or a pending submission is lost.
    pub worker_dir: PathBuf,
    /// Holds the worker's signing key.
    pub keyring: Arc<dyn keyring::Keyring>,
    /// Timeouts for requests and receipts.
    pub tuning: vera::AcpTuning,
}

impl fmt::Debug for VeraRsConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VeraRsConfig")
            .field("rpc_address", &self.rpc_address)
            .field("consensus_key", &self.consensus_key)
            .field("deployment_id", &self.deployment_id)
            .field("worker_dir", &self.worker_dir)
            .field("tuning", &self.tuning)
            .finish_non_exhaustive()
    }
}

impl PartialEq for VeraRsConfig {
    fn eq(&self, other: &Self) -> bool {
        self.rpc_address == other.rpc_address
            && self.consensus_key == other.consensus_key
            && self.deployment_id == other.deployment_id
            && self.actor_key == other.actor_key
            && self.worker_dir == other.worker_dir
            && Arc::ptr_eq(&self.keyring, &other.keyring)
            && self.tuning == other.tuning
    }
}

impl Eq for VeraRsConfig {}

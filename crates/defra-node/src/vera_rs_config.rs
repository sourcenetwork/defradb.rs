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
    /// The actor that owns this node's policies.
    pub actor: VeraRsActor,
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
            && self.actor == other.actor
            && self.worker_dir == other.worker_dir
            && Arc::ptr_eq(&self.keyring, &other.keyring)
            && self.tuning == other.tuning
    }
}

impl Eq for VeraRsConfig {}

/// Who a node's Vera policies belong to: the key that signs the delegations
/// the node's worker acts under.
#[derive(Clone)]
pub enum VeraRsActor {
    /// A 32-byte secp256k1 private key this process holds.
    Secp256k1Key(Vec<u8>),
    /// A signer for a key of any type vera.rs accepts (secp256k1, Ed25519,
    /// P-256), which may be held outside the process. The node builds every
    /// delegation itself and sends one only when the signer's signature
    /// verifies under its DID's key.
    Signer(Arc<dyn vera::vera_rs::DelegationSigner>),
}

impl fmt::Debug for VeraRsActor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Secp256k1Key(_) => formatter.write_str("Secp256k1Key(..)"),
            Self::Signer(signer) => formatter
                .debug_tuple("Signer")
                .field(&signer.did())
                .finish(),
        }
    }
}

impl PartialEq for VeraRsActor {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Secp256k1Key(a), Self::Secp256k1Key(b)) => a == b,
            (Self::Signer(a), Self::Signer(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for VeraRsActor {}

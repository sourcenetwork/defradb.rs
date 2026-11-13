//! Configuration of the Vera node a DefraDB node runs in its own process.

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub use vera_embedded::Deployment as EmbeddedVeraDeployment;

/// vera.rs document ACP from a single-validator Vera node the DefraDB node
/// starts before document ACP and stops on shutdown. The node is reached
/// in-process; every read is still verified against the deployment's
/// consensus key.
#[derive(Clone)]
pub struct EmbeddedVeraConfig {
    /// Directory the Vera node owns. It must not hold another store.
    pub data_dir: PathBuf,
    /// The deployment created on first start and reopened afterwards.
    pub deployment: EmbeddedVeraDeployment,
    /// The Vera node's consensus listener. `127.0.0.1:0` keeps it on
    /// loopback with a port the system picks.
    pub listen: SocketAddr,
    /// How long the build waits for the Vera node to start. A directory a
    /// node stopped earlier still holds is waited for until then.
    pub start_deadline: Duration,
    /// How long shutdown waits for the Vera node to stop. Past it,
    /// [`Node::shutdown`](crate::Node::shutdown) returns the failure and the
    /// directory stays held until the Vera node's thread ends.
    pub stop_deadline: Duration,
    /// The actor that owns this node's policies.
    pub actor: EmbeddedVeraActor,
    /// Where the worker journals a submission until Vera confirms it. It must
    /// survive restarts, or a pending submission is lost.
    pub worker_dir: PathBuf,
    /// Holds the worker's signing key.
    pub keyring: Arc<dyn keyring::Keyring>,
    /// Timeouts for requests and receipts.
    pub tuning: vera::AcpTuning,
}

/// Who an embedded Vera node's policies belong to: the key that signs the
/// delegations this node's worker acts under.
#[derive(Clone)]
pub enum EmbeddedVeraActor {
    /// A 32-byte secp256k1 private key this process holds.
    Secp256k1Key(Vec<u8>),
    /// A signer for a key of any type vera.rs accepts (secp256k1, Ed25519,
    /// P-256), which may be held outside the process. The node builds every
    /// delegation itself and sends one only when the signer's signature
    /// verifies under its DID's key.
    Signer(Arc<dyn vera::vera_rs::DelegationSigner>),
}

impl fmt::Debug for EmbeddedVeraActor {
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

impl PartialEq for EmbeddedVeraActor {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Secp256k1Key(a), Self::Secp256k1Key(b)) => a == b,
            (Self::Signer(a), Self::Signer(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for EmbeddedVeraActor {}

impl fmt::Debug for EmbeddedVeraConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddedVeraConfig")
            .field("data_dir", &self.data_dir)
            .field("deployment", &self.deployment)
            .field("listen", &self.listen)
            .field("start_deadline", &self.start_deadline)
            .field("stop_deadline", &self.stop_deadline)
            .field("actor", &self.actor)
            .field("worker_dir", &self.worker_dir)
            .field("tuning", &self.tuning)
            .finish_non_exhaustive()
    }
}

impl PartialEq for EmbeddedVeraConfig {
    fn eq(&self, other: &Self) -> bool {
        self.data_dir == other.data_dir
            && self.deployment == other.deployment
            && self.listen == other.listen
            && self.start_deadline == other.start_deadline
            && self.stop_deadline == other.stop_deadline
            && self.actor == other.actor
            && self.worker_dir == other.worker_dir
            && Arc::ptr_eq(&self.keyring, &other.keyring)
            && self.tuning == other.tuning
    }
}

impl Eq for EmbeddedVeraConfig {}

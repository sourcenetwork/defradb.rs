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
    /// How long shutdown waits for the Vera node to stop.
    pub stop_deadline: Duration,
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

impl fmt::Debug for EmbeddedVeraConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddedVeraConfig")
            .field("data_dir", &self.data_dir)
            .field("deployment", &self.deployment)
            .field("listen", &self.listen)
            .field("stop_deadline", &self.stop_deadline)
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
            && self.stop_deadline == other.stop_deadline
            && self.actor_key == other.actor_key
            && self.worker_dir == other.worker_dir
            && Arc::ptr_eq(&self.keyring, &other.keyring)
            && self.tuning == other.tuning
    }
}

impl Eq for EmbeddedVeraConfig {}

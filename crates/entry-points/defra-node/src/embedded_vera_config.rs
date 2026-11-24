//! Configuration of the Vera node a DefraDB node runs in its own process.

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub use vera_embedded::Deployment as EmbeddedVeraDeployment;
pub use vera_embedded::SeededPolicy as EmbeddedVeraPolicy;

pub use crate::vera_rs_config::VeraRsActor as EmbeddedVeraActor;

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
    /// Where the Vera node also serves its JSON-RPC methods, for another
    /// node of the same account that verifies against this deployment.
    /// `None` serves no listener.
    pub rpc_listen: Option<SocketAddr>,
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
            .field("start_deadline", &self.start_deadline)
            .field("stop_deadline", &self.stop_deadline)
            .field("actor", &self.actor)
            .field("rpc_listen", &self.rpc_listen)
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
            && self.rpc_listen == other.rpc_listen
            && self.worker_dir == other.worker_dir
            && Arc::ptr_eq(&self.keyring, &other.keyring)
            && self.tuning == other.tuning
    }
}

impl Eq for EmbeddedVeraConfig {}

/// The identity of a running embedded Vera deployment: what a verifying
/// client is configured with. It comes from the node that runs the
/// deployment, never from a node being asked over the network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddedVeraTrust {
    /// The deployment's chain identifier.
    pub chain_id: u64,
    /// Hex consensus key every proof and header is verified against.
    pub consensus_key: String,
}

//! The Vera node the transports reach, started and stopped by its host.
//!
//! The node builds a runtime and threads of its own: Commonware's runner
//! takes no runtime from outside. Its node thread is pinned to the network
//! runtime's CPUs before that runtime is built, so where a thread inherits
//! its creator's CPUs every thread of the node runs on the edge's cores and
//! none shares a core with a database worker.
//!
//! Stopping is modeled by proofs/tla/EmbeddedVeraStop.tla.

use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use commonware_codec::Encode as _;
use edge::Edge;
use vera_node::{EmbeddedConfig, EmbeddedNode, SoloGenesis, ThreadSetup};

pub use vera_node::ThreadBudget;

use crate::link::Link;
use crate::EmbeddedTransport;

/// The deployment a hosted node creates on first start and reopens later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deployment {
    /// Chain identifier, fixed when the directory is first used.
    pub chain_id: u64,
    /// How many distinct operator approvals an administrative change needs.
    pub threshold: u16,
    /// Operator keys in ascending string order: lowercase hexadecimal
    /// compressed secp256k1 keys, and `did:key` for Ed25519 and P-256 keys.
    pub operator_keys: Vec<String>,
    /// The time the deployment is created, in Unix seconds. Read on first
    /// start only.
    pub created: u64,
}

/// Where a hosted node keeps its state and how long its host waits for it.
#[derive(Clone, Debug)]
pub struct HostConfig {
    /// Directory the node owns. No other store shares it.
    pub data_dir: PathBuf,
    pub deployment: Deployment,
    /// Consensus listener. A lone validator dials no peer but still binds
    /// it; `127.0.0.1:0` keeps it on loopback with a port the system picks.
    pub listen: SocketAddr,
    /// How long [`HostedNode::start`] waits for the node to serve requests.
    /// A directory another node still holds is waited for until then.
    pub start_deadline: Duration,
    /// How long [`HostedNode::stop`] waits for the node's thread to finish.
    pub stop_deadline: Duration,
}

/// Why a hosted node did not start or did not stop.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HostError {
    #[error("invalid operator policy: {0}")]
    Operators(#[source] serde_json::Error),
    #[error("the embedded Vera node did not start: {0}")]
    Start(#[source] vera_node::EmbedError),
    #[error(
        "the embedded Vera node in {} did not start within {deadline:?}; another node may \
         still hold the directory",
        directory.display()
    )]
    StartElapsed {
        directory: PathBuf,
        deadline: Duration,
    },
    #[error("the embedded Vera node stopped: {0}")]
    Stopped(#[source] vera_node::EmbedError),
    #[error("the embedded Vera node did not stop within {0:?}")]
    StopElapsed(Duration),
    #[error("the network runtime ended the wait for the embedded Vera node: {0}")]
    Runtime(#[source] edge::EdgeError),
}

/// A running Vera node and the network runtime its host waits on.
#[derive(Debug)]
pub struct HostedNode {
    /// Taken by [`HostedNode::stop`].
    node: Option<EmbeddedNode>,
    link: Link,
    consensus_key: String,
    chain_id: u64,
    edge: Edge,
    threads: ThreadBudget,
    stop_deadline: Duration,
}

impl HostedNode {
    /// Creates or reopens the deployment and returns once the node serves
    /// requests. Its threads run on `edge`'s CPUs when `edge` holds cores of
    /// its own; a refused pin fails the start before the node builds its
    /// runtime.
    ///
    /// The wait ends at the start deadline. A node that has not started by
    /// then, for one because a node stopped past its own deadline still
    /// holds the directory, is told to stop and left to finish alone.
    pub async fn start(config: HostConfig, edge: &Edge) -> Result<Self, HostError> {
        Self::start_serving(config, None, edge).await
    }

    /// As [`start`](Self::start), with the node's JSON-RPC methods also
    /// served on `rpc_listen`, for a client in another process. What it reads
    /// there it verifies as it would from any Vera node.
    pub async fn start_serving(
        config: HostConfig,
        rpc_listen: Option<SocketAddr>,
        edge: &Edge,
    ) -> Result<Self, HostError> {
        let genesis = genesis(config.deployment)?;
        let directory = config.data_dir.clone();
        let mut settings = EmbeddedConfig::new(config.data_dir, genesis, config.listen);
        settings.rpc_listen = rpc_listen;
        // The node runs no more worker or verification threads than the
        // network runtime runs, pinned or not.
        let threads = edge.threads().max(1);
        settings.worker_threads = Some(threads);
        settings.verification_threads = settings.verification_threads.min(threads);
        if !edge.cpus().is_empty() {
            let placed = edge.clone();
            settings.thread_setup = Some(ThreadSetup::new(move || {
                if placed.pin_current_thread() {
                    Ok(())
                } else {
                    Err(std::io::Error::other(
                        "the node thread was refused the network runtime's CPUs",
                    ))
                }
            }));
        }
        let threads = settings.thread_budget();
        // Dropping the start at the deadline drops its stop sender, so a node
        // that gets its directory later stops by itself.
        let node = match edge.timeout(config.start_deadline, launch(settings)).await {
            Ok(started) => started.map_err(HostError::Start)?,
            Err(edge::EdgeError::Deadline(_)) => {
                return Err(HostError::StartElapsed {
                    directory,
                    deadline: config.start_deadline,
                })
            }
            Err(error) => return Err(HostError::Runtime(error)),
        };
        Ok(Self {
            link: Link::new(node.rpc().clone()),
            consensus_key: hex::encode(node.consensus_key().encode()),
            chain_id: node.genesis().chain_id,
            node: Some(node),
            edge: edge.clone(),
            threads,
            stop_deadline: config.stop_deadline,
        })
    }

    /// The in-process channel to this node. `request_timeout` bounds each
    /// submission call.
    pub fn transport(&self, request_timeout: Duration) -> EmbeddedTransport {
        EmbeddedTransport::linked(self.link.clone(), request_timeout, &self.edge)
    }

    /// The key finality proofs of this deployment verify against, in the
    /// hexadecimal form the provider takes as its trusted consensus key.
    pub fn consensus_key(&self) -> &str {
        &self.consensus_key
    }

    /// The deployment's chain identifier.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// The threads this node runs.
    pub fn threads(&self) -> ThreadBudget {
        self.threads
    }

    /// Stops the node and waits for its thread, at most the stop deadline.
    /// Its transports refuse every call from here on. At the deadline the
    /// node has been told to stop and is left to finish alone.
    pub async fn stop(mut self) -> Result<(), HostError> {
        self.link.close();
        let Some(node) = self.node.take() else {
            return Ok(());
        };
        match self.edge.timeout(self.stop_deadline, node.stop()).await {
            Ok(stopped) => stopped.map_err(HostError::Stopped),
            Err(edge::EdgeError::Deadline(_)) => Err(HostError::StopElapsed(self.stop_deadline)),
            Err(error) => Err(HostError::Runtime(error)),
        }
    }
}

/// A host dropped without [`HostedNode::stop`] tells the node to stop and
/// waits for nothing; its transports are closed all the same.
impl Drop for HostedNode {
    fn drop(&mut self) {
        self.link.close();
    }
}

/// The node's start, behind a pointer. Its future is nested deeper than the
/// compiler's default limit; built here, it is laid out in this crate and
/// not in every program that hosts a node.
#[inline(never)]
fn launch(
    settings: EmbeddedConfig,
) -> Pin<Box<dyn Future<Output = Result<EmbeddedNode, vera_node::EmbedError>> + Send>> {
    Box::pin(EmbeddedNode::start(settings))
}

fn genesis(deployment: Deployment) -> Result<SoloGenesis, HostError> {
    Ok(SoloGenesis {
        chain_id: deployment.chain_id,
        operators: serde_json::from_value(serde_json::json!({
            "threshold": deployment.threshold,
            "keys": deployment.operator_keys,
        }))
        .map_err(HostError::Operators)?,
        timestamp: deployment.created,
    })
}

//! The Vera node the transports reach, started and stopped by its host.
//!
//! The node builds a runtime and threads of its own: Commonware's runner
//! takes no runtime from outside. Its node thread is pinned to the network
//! runtime's CPUs before that runtime is built, so where a thread inherits
//! its creator's CPUs every thread of the node runs on the edge's cores and
//! none shares a core with a database worker.
//!
//! Stopping is modeled by proofs/tla/EmbeddedVeraStop.tla, and switching
//! the active device beside a stop by proofs/tla/EmbeddedVeraSwitch.tla.

mod device;
mod held;
mod release;
mod restart;
mod switch;

use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use commonware_codec::Encode as _;
use edge::Edge;
use futures::future::{select, Either};
use regolith::sync::Event;
use vera_node::{EmbeddedConfig, EmbeddedNode, SoloGenesis, ThreadSetup};

pub use device::HostDevice;
pub use vera_node::ThreadBudget;

use crate::link::Link;
use crate::EmbeddedTransport;
use held::Held;

/// How long a start waits before it asks again for a directory still held.
const DIRECTORY_RETRY: Duration = Duration::from_millis(50);

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
    /// Policies the deployment starts with. Part of its identity: a
    /// directory is reopened only with the list it was created with.
    pub policies: Vec<SeededPolicy>,
}

impl Deployment {
    /// Deal this deployment in memory, with a validator key and consensus
    /// share from operating-system randomness, for a host that keeps them
    /// in its keystore and starts its devices with [`HostDevice::Active`]
    /// and [`HostDevice::Backup`]. Nothing is written. `listen` is the
    /// consensus listener of the device that first runs it.
    pub fn deal(&self, listen: SocketAddr) -> Result<vera_node::SoloDeployment, HostError> {
        vera_node::SoloDeployment::create(&genesis(self.clone())?, listen)
            .map_err(HostError::Device)
    }
}

/// A policy a deployment holds from its first block. vera.rs derives its ID
/// from the definition, the creator and the nonce ([`SeededPolicy::id`]), so
/// the host knows the ID before the deployment exists, and a deployment of
/// another creator gets another ID for the same definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeededPolicy {
    /// The policy as YAML.
    pub definition: String,
    /// The nonce its ID is derived from. Zeroes are fine: the creator
    /// already tells one host's policies from another's.
    pub nonce: vera::PolicyNonce,
    /// The DID that created and owns the policy, part of its ID.
    pub creator: String,
}

impl SeededPolicy {
    /// The ID vera.rs gives the policy; the same ID the creator's own create
    /// of the definition under the nonce names, which then returns this
    /// policy and makes no second one.
    ///
    /// # Errors
    /// The definition does not compile.
    pub fn id(&self) -> Result<String, vera::ProviderError> {
        vera::vera_rs::predict_policy_id(&self.definition, self.nonce, &self.creator)
    }
}

/// Where a hosted node keeps its state and how long its host waits for it.
#[derive(Clone, Debug)]
pub struct HostConfig {
    /// Directory the node owns. No other store shares it.
    pub data_dir: PathBuf,
    /// The deployment a [`HostDevice::Alone`] node creates. The other
    /// devices read their deployment's identity from its record instead.
    pub deployment: Deployment,
    /// Which of the account's devices this node is.
    pub device: HostDevice,
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
    #[error("invalid seeded policy: {0}")]
    Policies(#[source] serde_json::Error),
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
    #[error("the embedded Vera node's directory cannot be checked for a node still running: {0}")]
    Directory(#[source] std::io::Error),
    #[error("the network runtime ended the wait for the embedded Vera node: {0}")]
    Runtime(#[source] edge::EdgeError),
    #[error("the embedded Vera node's deployment cannot be opened: {0}")]
    Device(#[source] vera_node::SoloError),
    #[error("the embedded Vera node holds no deployment to export: {0}")]
    Export(#[source] vera_node::SoloError),
    #[error("the embedded Vera node did not switch: {source}")]
    Switch {
        #[source]
        source: vera_node::EmbedError,
        /// Whether the node still serves requests: a refusal leaves it as it
        /// was; an error once it stopped for the switch does not.
        running: bool,
    },
    #[error(
        "the embedded Vera node did not switch within {deadline:?}; {}",
        if *running {
            "it runs in its earlier role"
        } else {
            "it is stopped, and HostedNode::complete_switch starts it in its new role"
        }
    )]
    SwitchElapsed { deadline: Duration, running: bool },
    #[error("the embedded Vera node is stopped")]
    NotRunning,
    #[error("the embedded Vera node is running; only a stopped node is started again")]
    Running,
    #[error(
        "the embedded Vera node started in {} belongs to another deployment; {}",
        directory.display(),
        match stop {
            None => "it was stopped".to_owned(),
            Some(error) => format!("stopping it failed: {error}"),
        }
    )]
    OtherDeployment {
        directory: PathBuf,
        /// Why that node did not stop within the stop deadline, if it did
        /// not. A stop of the host that begins meanwhile takes the wait over.
        #[source]
        stop: Option<Box<HostError>>,
    },
}

impl HostError {
    /// Whether the node did not start because the RPC address it was given
    /// could not be bound.
    pub const fn is_rpc_bind(&self) -> bool {
        matches!(self, Self::Start(vera_node::EmbedError::RpcBind { .. }))
    }

    /// Why the node's stored term refused a start or a switch, for a host
    /// that asks again (a backup not yet at its cut) or asks the user.
    /// Whether a refusal before the switch's certificate was accepted
    /// ([`node_runs`](Self::node_runs) then says the node runs on) or one
    /// after, which left the node stopped.
    pub const fn term_refusal(&self) -> Option<&vera_node::TermRefusal> {
        match self {
            Self::Start(vera_node::EmbedError::Term(refusal))
            | Self::Switch {
                source: vera_node::EmbedError::Term(refusal),
                ..
            } => Some(refusal),
            _ => None,
        }
    }

    /// Whether the hosted node still serves requests after the call that
    /// failed: a refused switch or handover leaves it as it was. `false`
    /// once it is stopped: a switch that failed after the node stopped for
    /// it, one a stop interrupted, and every error of a start or a stop.
    /// [`HostedNode::complete_switch`] or [`HostedNode::restart`] start a
    /// stopped node again, unless the host is stopping it.
    pub const fn node_runs(&self) -> bool {
        match self {
            Self::Switch { running, .. } | Self::SwitchElapsed { running, .. } => *running,
            Self::Export(_) | Self::Running => true,
            _ => false,
        }
    }

    /// A refusal of a call on a node that runs on.
    fn refused(source: vera_node::EmbedError) -> Self {
        Self::Switch {
            source,
            running: true,
        }
    }
}

/// A running Vera node and the network runtime its host waits on.
#[derive(Debug)]
pub struct HostedNode {
    /// Replaced by a switch, taken by [`HostedNode::stop`].
    held: Arc<Held>,
    /// Set once [`HostedNode::stop`] began: a switch or restart in progress
    /// is abandoned, and none starts a node after it.
    stopping: Event,
    /// The configuration the node was first started with, for a restart.
    /// The device the node runs as is the slot's.
    config: HostConfig,
    link: Link,
    consensus_key: String,
    chain_id: u64,
    edge: Edge,
    threads: ThreadBudget,
    rpc_addr: Option<SocketAddr>,
    start_deadline: Duration,
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
        let (settings, threads) = placed(&config, rpc_listen, edge)?;
        let node = match edge
            .timeout(config.start_deadline, start_held(settings, edge))
            .await
        {
            Ok(started) => started.map_err(HostError::Start)?,
            Err(edge::EdgeError::Deadline(_)) => {
                return Err(HostError::StartElapsed {
                    directory: config.data_dir.clone(),
                    deadline: config.start_deadline,
                })
            }
            Err(error) => return Err(HostError::Runtime(error)),
        };
        Ok(Self {
            link: Link::new(node.rpc().clone()),
            consensus_key: hex::encode(node.consensus_key().encode()),
            chain_id: node.genesis().chain_id,
            rpc_addr: node.rpc_addr(),
            held: Arc::new(Held::new(node, config.device.clone())),
            stopping: Event::new(),
            edge: edge.clone(),
            threads,
            start_deadline: config.start_deadline,
            stop_deadline: config.stop_deadline,
            config,
        })
    }

    /// `work`'s value, unless the start deadline passes first or a stop
    /// begins: either drops `work` where it stands. The stop is looked at
    /// before every step of `work`, so once it began `work` takes no other
    /// (proofs/tla/EmbeddedVeraSwitch.tla, AbortOnStop).
    async fn unless_stopped<F>(&self, work: F) -> Bounded<F::Output>
    where
        F: Future,
    {
        let timed = self.edge.timeout(self.start_deadline, work);
        let stop = self.stopping.wait();
        futures::pin_mut!(timed, stop);
        match select(stop, timed).await {
            Either::Left(_) => Bounded::Stopped,
            Either::Right((Ok(value), _)) => Bounded::Done(value),
            Either::Right((Err(edge::EdgeError::Deadline(_)), _)) => Bounded::Elapsed,
            Either::Right((Err(error), _)) => Bounded::Runtime(error),
        }
    }

    /// The in-process channel to this node, and to the node a switch starts
    /// in its place. `request_timeout` bounds each submission call.
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

    /// The bound JSON-RPC address when the host requested a listener. A
    /// switch keeps it.
    pub fn rpc_addr(&self) -> Option<SocketAddr> {
        self.rpc_addr
    }

    /// The threads this node runs.
    pub fn threads(&self) -> ThreadBudget {
        self.threads
    }

    /// Stops the node and waits for its thread, at most the stop deadline.
    /// Its transports refuse every call from here on, and every switching
    /// call fails with [`HostError::NotRunning`]. A switch or restart in
    /// progress is abandoned where it stands, as an interruption Vera
    /// recovers at the next start: a certificate it accepted is kept, and a
    /// node it was stopping or starting is told to stop. A handover, resume
    /// or takeover change in progress is waited for within the deadline.
    /// `Ok` means no Vera node thread runs in the node's directory any more:
    /// also one an abandoned switch or start left behind, which the stop
    /// waits for by the directory's lock. At the deadline the wait ends: a
    /// node the stop reached has been told to stop and finishes alone, and
    /// one a call still held is told to stop, by a task on the network
    /// runtime that waits one more stop deadline for the call to let go of
    /// it. Stopping again waits only for a thread still running there.
    pub async fn stop(&self) -> Result<(), HostError> {
        self.stopping.set();
        self.link.close();
        let stopped = async {
            let mut held = self.held.replace().await;
            let stopped = match held.node.take() {
                Some(node) => node.stop().await.map_err(HostError::Stopped),
                None => Ok(()),
            };
            let released = release::released(&self.config.data_dir, &self.edge)
                .await
                .map_err(HostError::Directory);
            stopped.and(released)
        };
        match self.edge.timeout(self.stop_deadline, stopped).await {
            Ok(stopped) => stopped,
            Err(edge::EdgeError::Deadline(_)) => {
                // Nobody else would take a node left in the slot. The reaper
                // gives up after one more stop deadline, so it holds up the
                // network runtime's drain no longer; a node it did not reach,
                // or that a closing runtime refused it, is told to stop when
                // the host drops the slot.
                let held = Arc::clone(&self.held);
                let edge = self.edge.clone();
                let deadline = self.stop_deadline;
                drop(self.edge.spawn(async move {
                    let reaped = async { drop(held.replace().await.node.take()) };
                    drop(edge.timeout(deadline, reaped).await);
                }));
                Err(HostError::StopElapsed(self.stop_deadline))
            }
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

/// How a switch or a restart bounded by the start deadline and the stop ended.
enum Bounded<T> {
    Done(T),
    Elapsed,
    Stopped,
    Runtime(edge::EdgeError),
}

/// The settings of the node `config` describes, its threads placed on
/// `edge`'s CPUs, and the threads it runs.
fn placed(
    config: &HostConfig,
    rpc_listen: Option<SocketAddr>,
    edge: &Edge,
) -> Result<(EmbeddedConfig, ThreadBudget), HostError> {
    let mut settings = device::settings(config)?;
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
    Ok((settings, threads))
}

/// Starts the node, asking again while another node holds the directory.
///
/// A node that was told to stop releases it shortly, so the caller bounds
/// this by its deadline. Dropping the start drops its stop sender, so a node
/// that got its directory stops by itself.
async fn start_held(
    settings: EmbeddedConfig,
    edge: &Edge,
) -> Result<EmbeddedNode, vera_node::EmbedError> {
    loop {
        match launch(settings.clone()).await {
            Err(vera_node::EmbedError::DirectoryInUse(_)) => {
                edge.sleep(DIRECTORY_RETRY).await;
            }
            started => return started,
        }
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
        policies: deployment
            .policies
            .into_iter()
            .map(|policy| {
                serde_json::from_value(serde_json::json!({
                    "definition": policy.definition,
                    "marshal_type": "ShortYaml",
                    "nonce": policy.nonce,
                    "owner": vera::vera_rs::did_spelling::wire(&policy.creator),
                }))
                .map_err(HostError::Policies)
            })
            .collect::<Result<_, _>>()?,
    })
}

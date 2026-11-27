//! Starting the hosted node again after a switch left it stopped.
//!
//! A take-over or a follow that failed once Vera stopped the node for it,
//! or that the start deadline cut short there, leaves the slot empty. The
//! certificate it accepted is stored, so a start in the new role completes
//! the switch. The node starts in the same slot, behind the same link, so
//! the provider built on the host's transports reaches it without being
//! built again.

use commonware_codec::Encode as _;
use futures::future::{select, Either};
use vera_node::EmbeddedNode;

use super::{placed, start_held, Bounded, HostConfig, HostDevice, HostError, HostedNode};

impl HostedNode {
    /// Start the node again in the role the last switch was heading to:
    /// after a take-over, as the active device with the keys it was given;
    /// after a follow, as the backup of the certificate's holder. For a
    /// switch that failed with the node stopped ([`HostError::node_runs`]
    /// is `false`). See [`restart`](Self::restart).
    pub async fn complete_switch(&self) -> Result<(), HostError> {
        self.start_again(None).await
    }

    /// Start the node again as `device`, of this deployment, in the slot a
    /// failed switch left empty, within the start deadline. The host's
    /// transports reach the new node, which serves JSON-RPC on the address
    /// the first node bound, if any.
    ///
    /// Fails with [`HostError::Running`] while a node runs, with
    /// [`HostError::NotRunning`] once a stop began (a stop that begins
    /// meanwhile abandons the start), and with
    /// [`HostError::OtherDeployment`] for a device of another deployment,
    /// whose node is stopped again within the stop deadline; the error says
    /// why if it did not stop. A start that fails or elapses leaves
    /// the slot empty, and may be asked again.
    pub async fn restart(&self, device: HostDevice) -> Result<(), HostError> {
        self.start_again(Some(device)).await
    }

    async fn start_again(&self, device: Option<HostDevice>) -> Result<(), HostError> {
        if self.stopping.is_set() {
            return Err(HostError::NotRunning);
        }
        let mut held = self.held.replace().await;
        if held.node.is_some() {
            return Err(HostError::Running);
        }
        let device = device.unwrap_or_else(|| held.device.clone());
        let config = HostConfig {
            device: device.clone(),
            ..self.config.clone()
        };
        let (settings, _) = placed(&config, self.rpc_addr, &self.edge)?;
        let earlier = std::mem::replace(&mut held.device, device);
        let node = match self.unless_stopped(start_held(settings, &self.edge)).await {
            Bounded::Done(started) => started.map_err(HostError::Start)?,
            Bounded::Elapsed => {
                return Err(HostError::StartElapsed {
                    directory: config.data_dir,
                    deadline: self.start_deadline,
                })
            }
            Bounded::Stopped => return Err(HostError::NotRunning),
            Bounded::Runtime(error) => return Err(HostError::Runtime(error)),
        };
        if hex::encode(node.consensus_key().encode()) != self.consensus_key {
            held.device = earlier;
            let stop = self.stop_foreign(node).await.err().map(Box::new);
            return Err(HostError::OtherDeployment {
                directory: config.data_dir,
                stop,
            });
        }
        self.link.open(node.rpc());
        held.node = Some(node);
        Ok(())
    }

    /// Stop a node of another deployment that a restart started, within the
    /// stop deadline. A stop of the host that begins meanwhile drops this
    /// wait, so it is not held up behind the slot: the node has been told to
    /// stop, and that stop waits for its thread by the node directory.
    async fn stop_foreign(&self, node: EmbeddedNode) -> Result<(), HostError> {
        let stopped = self.edge.timeout(self.stop_deadline, node.stop());
        let stopping = self.stopping.wait();
        futures::pin_mut!(stopped, stopping);
        match select(stopping, stopped).await {
            Either::Left(_) => Ok(()),
            Either::Right((Ok(stopped), _)) => stopped.map_err(HostError::Stopped),
            Either::Right((Err(edge::EdgeError::Deadline(_)), _)) => {
                Err(HostError::StopElapsed(self.stop_deadline))
            }
            Either::Right((Err(error), _)) => Err(HostError::Runtime(error)),
        }
    }
}

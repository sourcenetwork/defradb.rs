//! Moving the active role between the account's devices, through the host.
//!
//! Each call is the embedded node's own (`docs/active-term.md` in vera.rs),
//! made on the node the host runs. A take-over and a follow stop the node
//! and start another in its place; the host's transports then reach the new
//! node, so a provider built on them follows the switch.
//!
//! A switch that fails after the node stopped for it, or that the start
//! deadline or a stop interrupts there, leaves the slot empty with the
//! device it was heading to; [`HostedNode::complete_switch`] starts it.

use std::future::Future;
use std::pin::Pin;

use commonware_codec::Encode as _;
use futures::FutureExt as _;
use vera_domain::{PublicKey, TermCertificate, TermChange};
use vera_node::{Contest, DiscardedSubmission, EmbedError, EmbeddedNode, SoloDeployment, SoloKeys};

use super::held::Slot;
use super::{device, Bounded, HostConfig, HostDevice, HostError, HostedNode};

/// A switch made on the node: a future that borrows it to replace it.
type Switch<'node, T> = Pin<Box<dyn Future<Output = Result<T, EmbedError>> + Send + 'node>>;

impl HostedNode {
    /// The deployment's genesis record and key material, as
    /// [`HostDevice::Active`](super::HostDevice::Active) takes them on
    /// another device. Whoever holds the result can finalize blocks for the
    /// deployment. A backup holds no key material and fails.
    pub async fn export(&self) -> Result<SoloDeployment, HostError> {
        let slot = self.held.read().await;
        running(&slot)?.export().map_err(HostError::Export)
    }

    /// The key the account's other devices know this node by: the
    /// validator key on the active device, the transport key on a backup.
    pub async fn identity(&self) -> Result<PublicKey, HostError> {
        Ok(running(&*self.held.read().await)?.identity().clone())
    }

    /// Whether the node runs as the deployment's validator: as its only
    /// device or as the active one. `false` for a backup and once stopped.
    pub async fn is_active(&self) -> bool {
        let slot = self.held.read().await;
        slot.node.is_some() && !matches!(slot.device, HostDevice::Backup { .. })
    }

    /// The term this device holds. A deployment whose active device never
    /// changed is in term 0.
    pub async fn term(&self) -> Result<u64, HostError> {
        Ok(running(&*self.held.read().await)?.term())
    }

    /// The certificate of the newest term this device holds, verified
    /// against the operator policy in its own state. `None` in term 0.
    pub async fn term_certificate(&self) -> Result<Option<TermCertificate>, HostError> {
        Ok(running(&*self.held.read().await)?.term_certificate())
    }

    /// Announce that `successor`, one of the configured devices, takes the
    /// deployment over, and return the term change for the operators to
    /// approve. The node proposes nothing more until it follows the
    /// successor or [`resume`](Self::resume) undoes the announcement. A
    /// successor that is not connected or not current fails the call and
    /// the device stays active.
    pub async fn hand_over(&self, successor: &PublicKey) -> Result<TermChange, HostError> {
        let slot = self.held.read().await;
        running(&slot)?
            .hand_over(successor)
            .await
            .map_err(HostError::refused)
    }

    /// Undo [`hand_over`](Self::hand_over), for a handover whose certificate
    /// was never issued. Resuming after the successor received one forks the
    /// deployment.
    pub async fn resume(&self) -> Result<(), HostError> {
        let slot = self.held.read().await;
        running(&slot)?.resume().await.map_err(HostError::refused)
    }

    /// The term change a certificate must approve for this backup to take
    /// the deployment over without a handover. Blocks the active device
    /// finalized and this device never received are lost to the switch, so
    /// the host asks the user first.
    pub async fn takeover_change(&self) -> Result<TermChange, HostError> {
        let slot = self.held.read().await;
        running(&slot)?
            .takeover_change()
            .await
            .map_err(HostError::refused)
    }

    /// Become the deployment's validator, with `keys`, in the term
    /// `certificate` starts. The node restarts in place, within the start
    /// deadline, and the host's transports reach the new node.
    ///
    /// A refusal leaves the backup running. An error after the certificate
    /// was accepted, or the deadline, leaves the node stopped
    /// ([`HostError::node_runs`]); [`complete_switch`](Self::complete_switch)
    /// starts it as the active device. A stop that begins meanwhile
    /// abandons the switch.
    ///
    /// Submissions in flight on the backup were refused and stay owed by
    /// the document ACP provider; the node does not send them by itself
    /// until its next start, so a host sends them once the device is active
    /// (`defra_node::Node::take_over_embedded_vera` does).
    pub async fn take_over(
        &self,
        keys: SoloKeys,
        certificate: TermCertificate,
    ) -> Result<(), HostError> {
        let kept = keys.clone();
        let heading = move |device: &HostDevice| activated(device, &kept);
        self.switch(heading, move |node| {
            Box::pin(node.take_over(keys, certificate))
        })
        .await
    }

    /// Follow the device that holds the term `certificate` starts. The node
    /// restarts in place as its backup, within the start deadline, and the
    /// host's transports reach the new node. Returns the submissions this
    /// device finalized above the certificate's cut, which the term voided.
    ///
    /// A refusal leaves the node running. An error after the certificate was
    /// accepted, or the deadline, leaves the node stopped
    /// ([`HostError::node_runs`]); [`complete_switch`](Self::complete_switch)
    /// starts it as the holder's backup. A stop that begins meanwhile
    /// abandons the switch.
    ///
    /// A submission in flight when the node stops for the switch gets no
    /// answer, and one made after the switch is refused by the backup: both
    /// stay owed by the document ACP provider, and are sent once this
    /// device is active again.
    pub async fn follow(
        &self,
        certificate: TermCertificate,
    ) -> Result<Vec<DiscardedSubmission>, HostError> {
        let holder = certificate.change.holder;
        let heading = move |device: &HostDevice| following(device, holder.as_slice());
        self.switch(heading, move |node| Box::pin(node.follow(certificate)))
            .await
    }

    /// The finalized submissions that switches voided on this device and the
    /// host has not acknowledged, in the order they had executed.
    pub async fn discarded(&self) -> Result<Vec<DiscardedSubmission>, HostError> {
        running(&*self.held.read().await)?
            .discarded()
            .map_err(HostError::refused)
    }

    /// Forget the report [`discarded`](Self::discarded) returns.
    pub async fn acknowledge_discarded(&self) -> Result<(), HostError> {
        running(&*self.held.read().await)?
            .acknowledge_discarded()
            .map_err(HostError::refused)
    }

    /// Resolves when the active device learns that another device holds a
    /// later term, with that term's certificate; the host then calls
    /// [`follow`](Self::follow). It watches whichever node runs, across
    /// switches, and resolves to `None` once the node is stopped.
    pub async fn demoted(&self) -> Option<TermCertificate> {
        self.held.watch(|node| Box::pin(node.demoted())).await
    }

    /// Resolves when another device answers this active device as the
    /// deployment's validator and no certificate settles which of them holds
    /// the term. Like [`demoted`](Self::demoted), it watches across switches
    /// and resolves to `None` once the node is stopped.
    pub async fn contested(&self) -> Option<Contest> {
        self.held.watch(|node| Box::pin(node.contested())).await
    }

    /// Undo a handover the node in `config`'s directory announced before it
    /// stopped, for a handover whose certificate was never issued, so the
    /// device can start as the active device again. No node may run there.
    pub fn resume_stopped(config: &HostConfig) -> Result<(), HostError> {
        EmbeddedNode::resume_stopped(&device::settings(config)?).map_err(|source| {
            HostError::Switch {
                source,
                running: false,
            }
        })
    }

    /// Replace the node with the one `switch` starts in its place, and point
    /// the host's transports at it. `heading` names the device the switch
    /// makes of the slot's.
    async fn switch<T, H, S>(&self, heading: H, switch: S) -> Result<T, HostError>
    where
        H: FnOnce(&HostDevice) -> HostDevice,
        S: for<'node> FnOnce(&'node mut EmbeddedNode) -> Switch<'node, T>,
    {
        if self.stopping.is_set() {
            return Err(HostError::NotRunning);
        }
        let mut held = self.held.replace().await;
        let target = heading(&held.device);
        let node = held.node.as_mut().ok_or(HostError::NotRunning)?;
        // A stop that began, also before the slot was free, drops the switch
        // where it stands, as Vera recovers an interruption: the stop, or
        // the reaper it leaves at its deadline, takes whatever is left once
        // this call lets go.
        let ended = self.unless_stopped(switch(node)).await;
        let switched = matches!(ended, Bounded::Done(Ok(_)));
        let running = settle(&mut held, target, switched);
        match ended {
            Bounded::Done(Ok(value)) => {
                // The transports reach the node the switch started, unless a
                // stop closed them meanwhile.
                if let Some(node) = held.node.as_ref() {
                    self.link.open(node.rpc());
                }
                Ok(value)
            }
            Bounded::Done(Err(source)) => Err(HostError::Switch { source, running }),
            Bounded::Elapsed => Err(HostError::SwitchElapsed {
                deadline: self.start_deadline,
                running,
            }),
            Bounded::Stopped => Err(HostError::NotRunning),
            Bounded::Runtime(error) => Err(HostError::Runtime(error)),
        }
    }
}

/// Whether the slot's node still runs after a switch ended, however it
/// ended; one that does not leaves the slot, which then names `target`, the
/// device the switch was heading to. A switch that `switched` runs its node
/// as `target`, which the slot then names too; a refused one leaves the
/// node running as it was (proofs/tla/EmbeddedVeraSwitch.tla, ClearsHalted
/// and RecordsTarget).
///
/// Vera stops the node before it starts the next one in its place, and a
/// node it stopped, or whose start failed, is left halted: its exit has
/// been taken, so asking for it completes at once. A running node's exit
/// is still to come. A halted node is dropped unpolled again, as a
/// completed exit must not be awaited twice.
pub(super) fn settle(slot: &mut Slot, target: HostDevice, switched: bool) -> bool {
    let running = slot
        .node
        .as_mut()
        .is_some_and(|node| node.failed().now_or_never().is_none());
    if !running {
        slot.node = None;
    }
    if !running || switched {
        slot.device = target;
    }
    running
}

/// The device a take-over with `keys` makes of `device`.
fn activated(device: &HostDevice, keys: &SoloKeys) -> HostDevice {
    let (record, switching) = match device {
        HostDevice::Alone => return HostDevice::Alone,
        HostDevice::Active {
            deployment,
            switching,
        } => (deployment.record().to_vec(), switching),
        HostDevice::Backup {
            record, switching, ..
        } => (record.clone(), switching),
    };
    HostDevice::Active {
        deployment: Box::new(SoloDeployment::new(record, keys.clone())),
        switching: switching.clone(),
    }
}

/// The device following the holder `holder` (a certificate names a device
/// by its transport key's encoding) makes of `device`. A holder that is not
/// one of the account's devices is refused before the node stops, so the
/// device stays as it is.
fn following(device: &HostDevice, holder: &[u8]) -> HostDevice {
    let (record, switching) = match device {
        HostDevice::Alone => return HostDevice::Alone,
        HostDevice::Active {
            deployment,
            switching,
        } => (deployment.record().to_vec(), switching),
        HostDevice::Backup {
            record, switching, ..
        } => (record.clone(), switching),
    };
    let Some(active) = switching.as_ref().and_then(|switching| {
        switching
            .devices
            .iter()
            .find(|device| device.key.encode().as_ref() == holder)
            .map(|device| device.address)
    }) else {
        return device.clone();
    };
    HostDevice::Backup {
        record,
        active,
        switching: switching.clone(),
    }
}

fn running(slot: &Slot) -> Result<&EmbeddedNode, HostError> {
    slot.node.as_ref().ok_or(HostError::NotRunning)
}

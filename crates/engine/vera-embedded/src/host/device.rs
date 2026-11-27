//! The part a hosted node takes in its account's deployment, and the node
//! settings that part gives.

use std::net::SocketAddr;

use vera_node::{EmbeddedConfig, SoloDeployment, Switching};

use super::{genesis, HostConfig, HostError};

/// Which device of the account a hosted node is, and where its deployment's
/// key material lives.
#[derive(Clone, Debug, Default)]
pub enum HostDevice {
    /// The deployment's only device. The node creates the deployment from
    /// [`HostConfig::deployment`] on first start and keeps its validator key
    /// and consensus share in files in its directory. It cannot switch.
    #[default]
    Alone,
    /// The active device of a deployment whose key material the host holds,
    /// in its keystore. The node opens `deployment`, in an empty directory or
    /// in one that already holds it, signs from memory and writes neither the
    /// validator key nor the share. [`HostConfig::deployment`] is not read:
    /// the identity is the record's.
    Active {
        deployment: Box<SoloDeployment>,
        /// The account's devices, so the active role can move between them.
        /// `None` keeps this device active for good.
        switching: Option<Switching>,
    },
    /// A backup of the device that validates the deployment. It holds the
    /// deployment's genesis record and no key material, follows the active
    /// device it reaches at `active`, and serves reads and proofs from its
    /// own state. It refuses submissions with a retryable error.
    /// [`HostConfig::deployment`] is not read: the identity is the record's.
    Backup {
        /// The deployment's genesis record, as [`SoloDeployment::record`]
        /// returns it on the active device.
        record: Vec<u8>,
        /// The active device's consensus listener.
        active: SocketAddr,
        /// The account's devices, with this device's transport key. `None`
        /// keeps the transport key in the directory and follows for good.
        switching: Option<Switching>,
    },
}

impl HostDevice {
    /// The account's devices, when this device may switch.
    pub fn switching(&self) -> Option<&Switching> {
        match self {
            Self::Alone => None,
            Self::Active { switching, .. } | Self::Backup { switching, .. } => switching.as_ref(),
        }
    }
}

/// Two configurations of one device: the same role, the same record and key
/// material, and the same devices under the same transport key.
impl PartialEq for HostDevice {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Alone, Self::Alone) => true,
            (
                Self::Active {
                    deployment,
                    switching,
                },
                Self::Active {
                    deployment: theirs,
                    switching: their_switching,
                },
            ) => {
                deployment.record() == theirs.record()
                    && *deployment.keys().validator_key() == *theirs.keys().validator_key()
                    && *deployment.keys().share() == *theirs.keys().share()
                    && same_switching(switching.as_ref(), their_switching.as_ref())
            }
            (
                Self::Backup {
                    record,
                    active,
                    switching,
                },
                Self::Backup {
                    record: theirs,
                    active: their_active,
                    switching: their_switching,
                },
            ) => {
                record == theirs
                    && active == their_active
                    && same_switching(switching.as_ref(), their_switching.as_ref())
            }
            _ => false,
        }
    }
}

impl Eq for HostDevice {}

fn same_switching(ours: Option<&Switching>, theirs: Option<&Switching>) -> bool {
    match (ours, theirs) {
        (None, None) => true,
        (Some(ours), Some(theirs)) => {
            ours.device_key.public_key() == theirs.device_key.public_key()
                && ours.devices == theirs.devices
                && ours.freshness == theirs.freshness
                && ours.answer_timeout == theirs.answer_timeout
        }
        _ => false,
    }
}

/// The settings of the node `config` describes, before the host places its
/// threads.
pub(super) fn settings(config: &HostConfig) -> Result<EmbeddedConfig, HostError> {
    let mut settings = match &config.device {
        HostDevice::Alone => EmbeddedConfig::new(
            config.data_dir.clone(),
            genesis(config.deployment.clone())?,
            config.listen,
        ),
        HostDevice::Active {
            deployment,
            switching,
        } => {
            let mut settings = EmbeddedConfig::adopt(
                config.data_dir.clone(),
                SoloDeployment::clone(deployment),
                config.listen,
            )
            .map_err(HostError::Device)?;
            settings.switching = switching.clone();
            settings
        }
        HostDevice::Backup {
            record,
            active,
            switching,
        } => {
            let mut settings = EmbeddedConfig::backup(
                config.data_dir.clone(),
                record.clone(),
                config.listen,
                *active,
            )
            .map_err(HostError::Device)?;
            settings.switching = switching.clone();
            settings
        }
    };
    if let Some(pacing) = settings.pacing.as_mut() {
        pacing.batch = std::time::Duration::ZERO;
    }
    Ok(settings)
}

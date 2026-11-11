//! Who may connect in, and who may lift a revocation.
//!
//! Two gates, as on iroh. The allowlist answers "may this peer open a
//! connection to us" and is vinc's access hook. The revocation is vinc's own
//! bar (`Endpoint::revoke`), consulted in both directions and before the
//! hook, so a revoked peer is refused whatever the allowlist says. Both live
//! on the endpoint's thread; the facade reaches them through commands.

use std::cell::RefCell;
use std::rc::Rc;

use rapidhash::RapidHashSet;
use vinc::{Direction, NodeId};

use super::config::VincAllowlistConfig;
use crate::error::{Error, Result};

/// What a caller may do to the admission state, carried down to the change
/// rather than checked before it. Lifting a revocation needs both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmissionAuthority {
    may_admit: bool,
    may_revoke: bool,
}

impl AdmissionAuthority {
    pub fn new(may_admit: bool, may_revoke: bool) -> Self {
        Self {
            may_admit,
            may_revoke,
        }
    }

    /// May admit a peer, but may not lift a revocation.
    pub fn admit_only() -> Self {
        Self::new(true, false)
    }

    pub fn full() -> Self {
        Self::new(true, true)
    }

    pub(super) fn may_admit(self) -> bool {
        self.may_admit
    }

    pub(super) fn may_revoke(self) -> bool {
        self.may_revoke
    }
}

/// The inbound allowlist: everyone, or only the listed peers. Dials out are
/// never narrowed by it.
#[derive(Clone, Default)]
pub struct Allowlist {
    only: Rc<RefCell<Option<RapidHashSet<NodeId>>>>,
}

impl Allowlist {
    pub fn from_config(config: &VincAllowlistConfig) -> Result<Self> {
        let only = match config {
            VincAllowlistConfig::AcceptAll => None,
            VincAllowlistConfig::Explicit(ids) => Some(
                ids.iter()
                    .map(|id| {
                        vinc::node_from_str(&id.trim().to_ascii_lowercase()).map_err(|error| {
                            Error::InvalidConfig(format!(
                                "invalid vinc allowlist entry {id}: {error}"
                            ))
                        })
                    })
                    .collect::<Result<RapidHashSet<_>>>()?,
            ),
        };
        Ok(Self {
            only: Rc::new(RefCell::new(only)),
        })
    }

    /// Whether the allowlist is explicit and lists `peer`, or accepts all.
    pub fn admits_inbound(&self, peer: &NodeId) -> bool {
        self.only
            .try_borrow()
            .is_ok_and(|only| only.as_ref().is_none_or(|ids| ids.contains(peer)))
    }

    /// Add `peer`; a no-op when everyone is accepted.
    pub fn allow(&self, peer: NodeId) {
        if let Ok(mut only) = self.only.try_borrow_mut() {
            if let Some(ids) = only.as_mut() {
                ids.insert(peer);
            }
        }
    }

    /// Drop `peer`; whether it was listed.
    pub fn remove(&self, peer: &NodeId) -> bool {
        self.only
            .try_borrow_mut()
            .is_ok_and(|mut only| only.as_mut().is_some_and(|ids| ids.remove(peer)))
    }
}

impl vinc::Admission for Allowlist {
    fn admit(&self, peer: &NodeId, _alpn: &[u8], dir: Direction) -> bool {
        !matches!(dir, Direction::Inbound) || self.admits_inbound(peer)
    }
}

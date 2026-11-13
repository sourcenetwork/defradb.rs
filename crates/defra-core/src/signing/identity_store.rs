//! The process-wide signing identities, keyed by DID.
//!
//! A request names the identity that signs it by DID, so the signing keys
//! live here, where every caller can look one up. An identity is either
//! pinned, for the life of the process, or held: each holder (an FFI
//! identity handle, a node that generated or uses that identity) takes one
//! hold and gives it back, and the key leaves the store when the last hold
//! goes. Two holders may name one DID, so the store
//! counts them rather than letting the first release drop the other's key.
//!
//! While a DID has a key its signer is fixed: a hold that brings another
//! signer for the DID is refused, as is a pin over a held DID, so no holder
//! is re-routed to a signer it did not bring, and none keeps signing through
//! a signer whose owner already gave it up. Remote signers are compared by
//! the host context they call ([`RemoteSigner::host_context`](super::RemoteSigner::host_context)), so a second
//! registration of one context joins the hold rather than being refused.
//!
//! Modeled by `proofs/tla/IdentityStore.tla` and `proofs/lean/IdentityStore.lean`.

use std::sync::Arc;

use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use regolith::sync::{Atom, OnceCell};

use super::SigningConfig;

/// A hold or pin refused because the DID already has another signer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "identity {did} is already registered with a different signer; \
     free its existing identity handles first"
)]
pub struct IdentityConflict {
    /// The DID whose signer is taken.
    pub did: String,
}

/// One DID's entry. The count, the pin and the key change together in one
/// swap, so a release that reaches zero never drops a key a concurrent hold
/// just stored, and a hold never replaces a signer another holder uses.
#[derive(Clone, Default)]
struct Holders {
    pinned: bool,
    holds: usize,
    config: Option<SigningConfig>,
}

impl Holders {
    fn retained(&self) -> bool {
        self.pinned || self.holds > 0
    }
}

/// Whether `a` and `b` sign alike: the same key, and the same remote signer
/// (the same host callback and context) or none.
fn same_signer(a: &SigningConfig, b: &SigningConfig) -> bool {
    a.key_type == b.key_type
        && a.public_key_bytes == b.public_key_bytes
        && a.private_key_bytes == b.private_key_bytes
        && match (&a.remote_signer, &b.remote_signer) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b))
                    || a.host_context()
                        .is_some_and(|context| b.host_context() == Some(context))
            }
            _ => false,
        }
}

/// A DID's entry stays in the map once made: removing it could race a hold
/// that already fetched it. A released entry keeps no key, only its DID.
type Slot = Arc<Atom<Holders>>;

static IDENTITY_STORE: OnceCell<HopscotchMap<String, Slot, RandomState>> = OnceCell::new();

fn identity_store() -> &'static HopscotchMap<String, Slot, RandomState> {
    IDENTITY_STORE.get_or_init_racy(|| HopscotchMap::with_hasher(RandomState::default()))
}

fn slot(did: &str) -> Slot {
    let store = identity_store();
    store
        .get(did)
        .unwrap_or_else(|| store.get_or_insert(did.to_string(), Arc::default()))
}

/// Pins `did`'s signing config for the life of the process. A pin may
/// replace the signer of a DID only pinned; a DID something holds keeps its
/// signer, and a pin with another one is refused.
pub fn store_identity(did: &str, config: SigningConfig) -> Result<(), IdentityConflict> {
    let mut refused = false;
    slot(did).rcu(|current| {
        refused = current.holds > 0
            && current
                .config
                .as_ref()
                .is_some_and(|held| !same_signer(held, &config));
        if refused {
            return current.clone();
        }
        Holders {
            pinned: true,
            holds: current.holds,
            config: Some(config.clone()),
        }
    });
    match refused {
        true => Err(IdentityConflict {
            did: did.to_owned(),
        }),
        false => Ok(()),
    }
}

/// Stores `did`'s signing config and takes one hold on it, given back with
/// [`release_identity`]. Refused, taking nothing, when `did` already has a
/// key with another signer.
pub fn hold_identity(did: &str, config: SigningConfig) -> Result<(), IdentityConflict> {
    let mut refused = false;
    slot(did).rcu(|current| {
        refused = current
            .config
            .as_ref()
            .is_some_and(|stored| !same_signer(stored, &config));
        if refused {
            return current.clone();
        }
        Holders {
            pinned: current.pinned,
            holds: current.holds + 1,
            config: current.config.clone().or_else(|| Some(config.clone())),
        }
    });
    match refused {
        true => Err(IdentityConflict {
            did: did.to_owned(),
        }),
        false => Ok(()),
    }
}

/// One hold on a DID's signing config, given back when dropped.
#[must_use = "dropping the hold gives it back at once"]
pub struct IdentityHold {
    did: String,
}

impl IdentityHold {
    /// Stores `did`'s signing config and holds it until this is dropped.
    /// Refused as [`hold_identity`] is.
    pub fn new(did: &str, config: SigningConfig) -> Result<Self, IdentityConflict> {
        hold_identity(did, config)?;
        Ok(Self {
            did: did.to_owned(),
        })
    }
}

impl Drop for IdentityHold {
    fn drop(&mut self) {
        release_identity(&self.did);
    }
}

/// Takes one more hold on `did`'s stored signing config. Answers false, and
/// takes nothing, when `did` has none.
pub fn retain_identity(did: &str) -> bool {
    let Some(slot) = identity_store().get(did) else {
        return false;
    };
    let mut retained = false;
    slot.rcu(|current| {
        retained = current.config.is_some();
        Holders {
            holds: current.holds + usize::from(retained),
            ..current.clone()
        }
    });
    retained
}

/// Gives back one hold on `did`. The last hold on an identity that is not
/// pinned removes its signing config, and with it the private key.
pub fn release_identity(did: &str) {
    let Some(slot) = identity_store().get(did) else {
        return;
    };
    slot.rcu(|current| {
        let mut next = Holders {
            holds: current.holds.saturating_sub(1),
            ..current.clone()
        };
        if !next.retained() {
            next.config = None;
        }
        next
    });
}

/// Retrieve stored signing config for a DID.
pub fn get_identity(did: &str) -> Option<SigningConfig> {
    identity_store()
        .get(did)
        .and_then(|slot| slot.peek(|holders| holders.config.clone()))
}

/// Find a registered DID backed by a remote signer.
///
/// This is used by HTTP request handling when the authenticated caller DID has no
/// locally registered signing identity. In Orbis-backed deployments, the node
/// should fall back to the remote signer identity rather than a local `--identity`
/// secp256k1 key, so create mutations still go through the signing gate.
pub fn find_remote_signer_did() -> Option<String> {
    identity_store().iter().find_map(|(did, slot)| {
        slot.peek(|holders| {
            holders
                .config
                .as_ref()
                .is_some_and(SigningConfig::has_remote_signer)
        })
        .then_some(did)
    })
}

/// Unpins every identity and drops every key nothing holds. A held key stays
/// until its last hold is given back, and every slot stays in the map: a
/// holder gives its hold back by DID, and must find the slot it counted on.
pub fn clear_identity_store() {
    for (_, slot) in identity_store().iter() {
        slot.rcu(|current| {
            let mut next = Holders {
                pinned: false,
                ..current.clone()
            };
            if !next.retained() {
                next.config = None;
            }
            next
        });
    }
}

#[cfg(test)]
#[path = "identity_store_tests.rs"]
mod tests;

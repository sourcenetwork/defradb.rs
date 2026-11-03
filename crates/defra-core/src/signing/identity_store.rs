//! The process-wide signing identities, keyed by DID.
//!
//! A request names the identity that signs it by DID, so the signing keys
//! live here, where every caller can look one up. An identity is either
//! pinned, for the life of the process (a node's own generated identity, the
//! CLI's), or held: each holder (an FFI identity handle, a node using that
//! identity) takes one hold and gives it back, and the key leaves the store
//! when the last hold goes. Two holders may name one DID, so the store
//! counts them rather than letting the first release drop the other's key.

use std::sync::Arc;

use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use regolith::sync::{Atom, OnceCell};

use super::SigningConfig;

/// One DID's entry. The count and the key change together in one swap, so a
/// release that reaches zero never drops a key a concurrent hold just stored.
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

/// Pins `did`'s signing config for the life of the process.
pub fn store_identity(did: &str, config: SigningConfig) {
    slot(did).rcu(|current| Holders {
        pinned: true,
        holds: current.holds,
        config: Some(config.clone()),
    });
}

/// Stores `did`'s signing config and takes one hold on it, given back with
/// [`release_identity`].
pub fn hold_identity(did: &str, config: SigningConfig) {
    slot(did).rcu(|current| Holders {
        pinned: current.pinned,
        holds: current.holds + 1,
        config: Some(config.clone()),
    });
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

/// Clear all stored identities (for node cleanup).
pub fn clear_identity_store() {
    identity_store().clear();
}

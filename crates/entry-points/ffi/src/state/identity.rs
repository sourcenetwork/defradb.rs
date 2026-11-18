use std::sync::{Arc, Weak};

use defra_core::signing::{IdentityHold, SigningConfig, SigningKeyType};
use identity::{Identity, RawIdentity};
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;
use regolith::sync::Lazy;
use zeroize::Zeroizing;

/// Type alias for identity handles (opaque to FFI callers).
pub type IdentityHandle = usize;

/// An identity a C caller holds a handle to: a key pair, or a public key
/// whose private key the host keeps, and the DID its public key derives.
pub struct IdentityState {
    /// The identity's `did:key`.
    pub did: String,
    /// The identity's key type; `None` for one a bearer token stands for.
    pub key_type: Option<SigningKeyType>,
    /// The raw private key; `None` when the host keeps it (a remote signer).
    pub private_key: Option<Zeroizing<Vec<u8>>>,
    /// The hex-encoded public key.
    pub public_key_hex: String,
    /// The hold on the identity's signing config, given back when the last
    /// reference to this state goes, so a caller still using the state (one
    /// handing it to a node) keeps the key even if the handle is freed
    /// meanwhile. `None` for an identity a bearer token stands for.
    pub hold: Option<IdentityHold>,
    /// The token an identity a bearer token stands for acts by. It travels
    /// with each request that names this identity, never through a
    /// process-wide store, so it goes when the last reference to this state
    /// does, and two handles for one DID each present their own.
    pub bearer_token: Option<Zeroizing<String>>,
}

impl IdentityState {
    /// The state of `identity`, which holds its key in the signing store under
    /// its DID, which is how a request names the identity that signs it.
    pub fn register(identity: &RawIdentity) -> Result<Self, String> {
        let did = identity
            .did()
            .map_err(|error| format!("failed to derive DID: {}", error))?
            .to_string();
        let key_type = match identity.key_type() {
            crypto::KeyType::Ed25519 => SigningKeyType::Ed25519,
            crypto::KeyType::Secp256k1 => SigningKeyType::Secp256k1,
            crypto::KeyType::Secp256r1 => SigningKeyType::Secp256r1,
            other => return Err(format!("unsupported identity key type: {}", other)),
        };
        let private_key = Zeroizing::new(identity.private_key_bytes());
        let public_key_bytes = identity.public_key_bytes();
        let public_key_hex = hex::encode(&public_key_bytes);

        let hold = IdentityHold::new(
            &did,
            SigningConfig {
                key_type,
                private_key_bytes: SigningConfig::private_key_bytes_from_vec(private_key.to_vec()),
                public_key_bytes,
                public_key_hex: public_key_hex.clone(),
                remote_signer: None,
                signing_authorization: None,
            },
        )
        .map_err(|error| error.to_string())?;

        Ok(Self {
            did,
            key_type: Some(key_type),
            private_key: Some(private_key),
            public_key_hex,
            hold: Some(hold),
            bearer_token: None,
        })
    }
}

/// A registered identity: the handle's owning reference plus a borrowing one.
///
/// The map retires a removed entry instead of dropping it, so the owning
/// reference sits in a queue that hands it back by value at `release`.
/// Freeing a handle then gives back its hold on the key at once, rather than
/// whenever reclamation catches up.
struct IdentitySlot {
    owner: SegQueue<Arc<IdentityState>>,
    reader: Weak<IdentityState>,
}

/// Global registry of identities.
pub struct IdentityRegistry {
    identities: HopscotchMap<IdentityHandle, Arc<IdentitySlot>, RandomState>,
}

impl IdentityRegistry {
    fn new() -> Self {
        Self {
            identities: HopscotchMap::with_hasher(RandomState::default()),
        }
    }

    /// Insert an identity and return its handle.
    pub fn insert(&self, state: IdentityState) -> IdentityHandle {
        let handle = super::handle::next_handle();
        let state = Arc::new(state);
        let slot = IdentitySlot {
            owner: SegQueue::new(),
            reader: Arc::downgrade(&state),
        };
        slot.owner.push(state);
        self.identities.insert(handle, Arc::new(slot));
        handle
    }

    /// The identity a handle names, if it names one. Holding the returned
    /// state keeps the identity's key in the signing store after the handle
    /// is freed.
    pub fn get(&self, handle: IdentityHandle) -> Option<Arc<IdentityState>> {
        self.identities.get(&handle)?.reader.upgrade()
    }

    /// Releases a handle. Its hold on the identity's key goes with the last
    /// reference to the identity's state, and the key leaves the signing store
    /// once no other handle or node holds it.
    pub fn release(&self, handle: IdentityHandle) {
        if let Some(slot) = self.identities.remove(&handle) {
            drop(slot.owner.pop());
        }
    }
}

/// A node's signing identity: its DID, and whether the node holds that DID's
/// key in the signing store, so freeing the identity's handle leaves the node
/// able to sign.
#[derive(Clone)]
pub struct NodeIdentity {
    pub did: String,
    held: bool,
}

impl NodeIdentity {
    /// `did`, with a hold on its key when the signing store has one.
    pub fn hold(did: String) -> Self {
        let held = defra_core::signing::retain_identity(&did);
        Self { did, held }
    }

    /// Gives back the node's hold on the key.
    pub fn release(&self) {
        if self.held {
            defra_core::signing::release_identity(&self.did);
        }
    }
}

/// Global IDENTITIES registry.
pub static IDENTITIES: Lazy<IdentityRegistry> = Lazy::new(IdentityRegistry::new);

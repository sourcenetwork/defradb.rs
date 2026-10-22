//! Which key a record advertises a collection under.
//!
//! Collection ids come from the schema, so anyone with the schema can work
//! them out. A collection is advertised by a keyed hash of its id under the
//! network secret, so only nodes sharing the secret can match it, unless it
//! is public. Proof: `proofs/lean/ServiceRecord/Blinding.lean`.

use serde::{Deserialize, Serialize};

/// Keeps these hashes apart from any other use of the network secret.
const LABEL: &[u8] = b"dsr1";

/// The 32-byte secret every node of one network shares.
#[derive(Clone, PartialEq, Eq)]
pub struct NetworkSecret([u8; 32]);

impl NetworkSecret {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Reads the secret from 64 hex characters, as the config holds it.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim();
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (byte, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks(2)) {
            *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl std::fmt::Debug for NetworkSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NetworkSecret([REDACTED])")
    }
}

/// A collection as a record names it. The variant order is the wire tag:
/// a blinded key and a plain one never encode alike.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CollectionKey {
    Blinded([u8; 32]),
    Plain(String),
}

impl CollectionKey {
    /// The bytes the book indexes this key by.
    pub fn encode(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("a collection key always encodes")
    }
}

/// The blinded key of `collection_id` under `secret`.
pub fn blind(secret: &NetworkSecret, collection_id: &str) -> CollectionKey {
    let mut hasher = blake3::Hasher::new_keyed(&secret.0);
    hasher.update(LABEL);
    hasher.update(collection_id.as_bytes());
    CollectionKey::Blinded(*hasher.finalize().as_bytes())
}

/// The key a node advertises for a collection: its plain id when public,
/// its blinded key when the node has a secret, nothing otherwise.
pub fn advertised_key(
    secret: Option<&NetworkSecret>,
    public: bool,
    collection_id: &str,
) -> Option<CollectionKey> {
    if public {
        return Some(CollectionKey::Plain(collection_id.to_owned()));
    }
    secret.map(|secret| blind(secret, collection_id))
}

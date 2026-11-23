//! A peer key the caller supplies for one start, as Go's `P2PPrivateKey`.

use std::fmt;

use zeroize::Zeroizing;

use crate::{Error, Result};

/// A peer key for one start: used as given, never checked against the key
/// the store holds and never recorded in it, so a later start without it
/// carries on with the store's own key.
///
/// Modeled by `proofs/tla/PeerKeyLoad.tla` and `proofs/lean/PeerKeyLoad.lean`.
#[derive(Clone, PartialEq, Eq)]
pub struct StartPeerKey(Zeroizing<[u8; 32]>);

impl StartPeerKey {
    /// The start key an Ed25519 private key's bytes name: its 32-byte seed,
    /// or the 64-byte seed and public key.
    pub fn from_ed25519_private_key(bytes: &[u8]) -> Result<Self> {
        use crypto::Key;
        let key = crypto::Ed25519PrivateKey::from_bytes(bytes)
            .map_err(|error| Error::InvalidStartPeerKey(error.to_string()))?;
        let raw = key.raw();
        let seed: [u8; 32] = raw
            .get(..32)
            .and_then(|seed| seed.try_into().ok())
            .ok_or_else(|| Error::InvalidStartPeerKey(format!("{} bytes", raw.len())))?;
        Ok(Self(Zeroizing::new(seed)))
    }

    /// The key's 32-byte Ed25519 seed, which the P2P transports start with.
    #[cfg(any(feature = "p2p", feature = "libp2p"))]
    pub(crate) fn seed(&self) -> &Zeroizing<[u8; 32]> {
        &self.0
    }
}

impl fmt::Debug for StartPeerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StartPeerKey(<redacted>)")
    }
}

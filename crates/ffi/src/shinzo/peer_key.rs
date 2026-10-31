//! A peer key the caller supplies (Go's `P2P().SetPrivateKey`), held as the
//! node's peer keyring, so every start has the peer ID that key gives.

use std::sync::Arc;

use zeroize::Zeroizing;

use crate::types::NodeInitOptions;

/// The Ed25519 key length Go's libp2p `PrivKey.Raw()` gives: seed, then
/// public key.
const KEY_LEN: usize = 64;

/// The peer keyring `options.p2p_private_key` names, or none.
///
/// # Safety
///
/// `options.p2p_private_key` must be null or point to
/// `options.p2p_private_key_len` readable bytes.
pub unsafe fn peer_keyring(
    options: &NodeInitOptions,
) -> Result<Option<defra_node::PeerKeyring>, String> {
    if options.p2p_private_key.is_null() || options.p2p_private_key_len == 0 {
        return Ok(None);
    }
    if options.p2p_private_key_len != KEY_LEN {
        return Err(format!(
            "p2p_private_key must be {KEY_LEN} bytes, got {}",
            options.p2p_private_key_len
        ));
    }
    // SAFETY: non-null and exactly KEY_LEN bytes, per the caller.
    let key = unsafe { std::slice::from_raw_parts(options.p2p_private_key, KEY_LEN) };
    crypto::Ed25519PrivateKey::from_bytes(key)
        .map_err(|error| format!("invalid p2p_private_key: {error}"))?;
    Ok(Some(defra_node::PeerKeyring::new(Arc::new(FixedPeerKey(
        Zeroizing::new(key.to_vec()),
    )))))
}

/// A keyring holding only the peer key it was given. The node never needs
/// to store one, since it reads this key on every start.
struct FixedPeerKey(Zeroizing<Vec<u8>>);

impl keyring::Keyring for FixedPeerKey {
    fn set(&self, name: &str, key: &[u8]) -> keyring::Result<()> {
        match name == keyring::PEER_KEY && key == self.0.as_slice() {
            true => Ok(()),
            false => Err(keyring::Error::InvalidKeyName(name.to_owned())),
        }
    }

    fn get(&self, name: &str) -> keyring::Result<Zeroizing<Vec<u8>>> {
        match name == keyring::PEER_KEY {
            true => Ok(self.0.clone()),
            false => Err(keyring::Error::NotFound(name.to_owned())),
        }
    }

    fn list(&self) -> keyring::Result<Vec<String>> {
        Ok(vec![keyring::PEER_KEY.to_owned()])
    }

    fn delete(&self, name: &str) -> keyring::Result<()> {
        Err(keyring::Error::InvalidKeyName(name.to_owned()))
    }
}

//! A peer key the C caller supplies, served to the node as its keyring.

use defra_node::keyring;
use zeroize::Zeroizing;

/// A keyring holding only the caller's P2P private key, which the node reads
/// as its peer key instead of the one in its store.
pub(crate) struct CallerPeerKey(Zeroizing<Vec<u8>>);

impl CallerPeerKey {
    pub(crate) fn new(key: Zeroizing<Vec<u8>>) -> Self {
        Self(key)
    }
}

impl defra_node::Keyring for CallerPeerKey {
    fn set(&self, name: &str, key: &[u8]) -> keyring::Result<()> {
        if name == keyring::PEER_KEY && key == self.0.as_slice() {
            Ok(())
        } else {
            Err(keyring::Error::InvalidKeyName(name.to_string()))
        }
    }

    fn get(&self, name: &str) -> keyring::Result<Zeroizing<Vec<u8>>> {
        if name == keyring::PEER_KEY {
            Ok(self.0.clone())
        } else {
            Err(keyring::Error::NotFound(name.to_string()))
        }
    }

    fn delete(&self, name: &str) -> keyring::Result<()> {
        Err(keyring::Error::InvalidKeyName(name.to_string()))
    }

    fn list(&self) -> keyring::Result<Vec<String>> {
        Ok(vec![keyring::PEER_KEY.to_string()])
    }
}

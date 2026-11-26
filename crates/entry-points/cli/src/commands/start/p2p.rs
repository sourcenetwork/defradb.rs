//! The node's peer key.

use tracing::info;
use zeroize::Zeroizing;

use super::node::Node;
use crate::config::Config;
use crate::error::{Error, Result};

impl Node {
    /// Load the peer key's Ed25519 seed from the keyring, or generate a key
    /// and store it there first.
    ///
    /// The keyring holds 64 bytes, the 32-byte seed then the 32-byte public
    /// key; the seed is what the pinc peer id comes from.
    pub(super) fn init_peer_key(config: &Config) -> Result<Zeroizing<[u8; 32]>> {
        use keyring::PEER_KEY;

        let kr = crate::commands::open_keyring(config)?;

        match kr.get(PEER_KEY) {
            Ok(key_bytes) => {
                info!("Loaded existing peer key from keyring");
                Self::seed_from_ed25519_bytes(&key_bytes)
            }
            Err(keyring::Error::NotFound(_)) => {
                info!("Generating new peer key");
                use crypto::Key;
                let private_key = crypto::generate_ed25519()
                    .map_err(|e| Error::Keyring(format!("failed to generate peer key: {}", e)))?;
                let key_bytes = private_key.raw();

                kr.set(PEER_KEY, key_bytes)
                    .map_err(|e| Error::Keyring(e.to_string()))?;

                Self::seed_from_ed25519_bytes(key_bytes)
            }
            Err(e) => Err(Error::Keyring(e.to_string())),
        }
    }

    /// The 32-byte seed at the start of a 64-byte Ed25519 key.
    fn seed_from_ed25519_bytes(key_bytes: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
        if key_bytes.len() != 64 {
            return Err(Error::Keyring(format!(
                "invalid peer key length: expected 64 bytes, got {}",
                key_bytes.len()
            )));
        }
        let seed: [u8; 32] = key_bytes[..32]
            .try_into()
            .map_err(|_| Error::Keyring("invalid key format".to_string()))?;
        Ok(Zeroizing::new(seed))
    }
}

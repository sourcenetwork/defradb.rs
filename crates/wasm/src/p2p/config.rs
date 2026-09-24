//! How a page configures its peer, and the endpoint key that follows from it.

use hkdf::Hkdf;
use identity::RawIdentity;
use p2p::iroh::{IrohDiscoveryConfig, IrohRelayModeConfig};
use serde::Deserialize;
use sha2::Sha256;
use zeroize::Zeroize;

use crate::error::{Result, WasmError};

/// Domain separation for the endpoint key derived from a client identity, so
/// the same key material never serves as both a signing key and a QUIC key.
const ENDPOINT_KEY_DOMAIN: &[u8] = b"defradb-wasm-iroh-endpoint";

#[derive(Debug, Default, Deserialize)]
pub(crate) struct P2PConfig {
    /// Relays to reach peers through. Empty uses iroh's default relays.
    #[serde(default)]
    pub(crate) relay_urls: Vec<String>,
    /// A 32-byte endpoint key as hex, overriding the one derived from the
    /// client identity.
    #[serde(default)]
    pub(crate) secret_key_hex: Option<String>,
    /// Publish and resolve addresses through the n0 pkarr relay. Publishing
    /// announces the endpoint id to a third party, so it is off unless asked.
    #[serde(default)]
    pub(crate) discovery: bool,
}

impl P2PConfig {
    pub(crate) fn relay_mode(&self) -> IrohRelayModeConfig {
        if self.relay_urls.is_empty() {
            IrohRelayModeConfig::Default
        } else {
            IrohRelayModeConfig::Custom(self.relay_urls.clone())
        }
    }

    pub(crate) fn discovery(&self) -> IrohDiscoveryConfig {
        if self.discovery {
            IrohDiscoveryConfig::N0
        } else {
            IrohDiscoveryConfig::Disabled
        }
    }

    /// The endpoint key: an explicit one, else one derived from the identity so
    /// a peer that stored this browser as a replicator can still reach it after
    /// a reload, else an ephemeral one.
    pub(crate) fn secret_key(&self, identity: Option<&RawIdentity>) -> Result<iroh::SecretKey> {
        if let Some(hex_key) = self.secret_key_hex.as_deref() {
            return Ok(p2p::iroh::secret_key_from_bytes(parse_secret_key(hex_key)?));
        }
        Ok(match identity {
            Some(identity) => p2p::iroh::secret_key_from_bytes(derive_endpoint_key(identity)),
            None => p2p::iroh::generate_secret_key(),
        })
    }
}

fn derive_endpoint_key(identity: &RawIdentity) -> [u8; 32] {
    let mut private_key = identity.private_key_bytes();
    let mut key = [0u8; 32];
    Hkdf::<Sha256>::new(None, &private_key)
        .expand(ENDPOINT_KEY_DOMAIN, &mut key)
        .expect("32 bytes is within HKDF-SHA256's output limit");
    private_key.zeroize();
    key
}

fn parse_secret_key(hex_key: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_key.trim())
        .map_err(|error| WasmError::InvalidArgument(format!("secret key is not hex: {error}")))?;
    bytes
        .try_into()
        .map_err(|_| WasmError::InvalidArgument("secret key must be exactly 32 bytes".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn identity(seed: u8) -> RawIdentity {
        let key = crypto::ed25519_key_from_seed(&[seed; 32]).unwrap();
        RawIdentity::from_identity_key_type(identity::IdentityKeyType::Ed25519, &key).unwrap()
    }

    #[wasm_bindgen_test]
    fn an_identity_fixes_the_endpoint_key() {
        let config = P2PConfig::default();
        let first = config.secret_key(Some(&identity(1))).unwrap();
        let second = config.secret_key(Some(&identity(1))).unwrap();
        let other = config.secret_key(Some(&identity(2))).unwrap();

        assert_eq!(first.public(), second.public());
        assert_ne!(first.public(), other.public());
    }

    #[wasm_bindgen_test]
    fn the_endpoint_key_is_not_the_identity_key() {
        let identity = identity(3);
        let derived = P2PConfig::default().secret_key(Some(&identity)).unwrap();
        assert_ne!(
            derived.to_bytes().to_vec(),
            identity.private_key_bytes()[..32]
        );
    }

    #[wasm_bindgen_test]
    fn an_explicit_key_overrides_the_identity() {
        let config = P2PConfig {
            secret_key_hex: Some("01".repeat(32)),
            ..Default::default()
        };
        let explicit = config.secret_key(Some(&identity(4))).unwrap();
        assert_eq!(explicit.to_bytes(), [1u8; 32]);
    }

    #[wasm_bindgen_test]
    fn a_malformed_key_is_rejected() {
        for bad in ["0102", "zz"] {
            let config = P2PConfig {
                secret_key_hex: Some(bad.into()),
                ..Default::default()
            };
            assert!(matches!(
                config.secret_key(None),
                Err(WasmError::InvalidArgument(_))
            ));
        }
    }
}

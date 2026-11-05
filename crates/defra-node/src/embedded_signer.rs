use defra_core::signing::{SigningConfig, SigningKeyType};
use identity::{Identity as _, IdentityKeyType, RawIdentity};

/// A local signing capability deliberately supplied by an embedded caller.
///
/// The caller is responsible for possessing and authorizing the key; a DID or
/// membership in the process-wide identity registry is not a capability. The
/// request actor must match this key, and native ACP still authorizes the actor.
#[derive(Clone)]
pub struct EmbeddedSigner {
    did: String,
    config: SigningConfig,
}

/// Why a local signing capability cannot be constructed.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddedSignerError {
    #[error("embedded signing requires a local Ed25519 key without a remote signer")]
    UnsupportedSigner,
    #[error("invalid Ed25519 private key")]
    InvalidPrivateKey,
    #[error("private and public signing keys do not match")]
    PublicKeyMismatch,
    #[error("cannot derive a DID from the signing key")]
    InvalidDid,
}

impl EmbeddedSigner {
    /// Capture and validate an existing local key without looking it up by DID.
    pub fn from_config(config: SigningConfig) -> Result<Self, EmbeddedSignerError> {
        if config.key_type != SigningKeyType::Ed25519
            || !config.has_local_private_key()
            || config.has_remote_signer()
            || config.signing_authorization.is_some()
        {
            return Err(EmbeddedSignerError::UnsupportedSigner);
        }
        let identity = RawIdentity::from_identity_key_type(
            IdentityKeyType::Ed25519,
            &config.private_key_bytes,
        )
        .map_err(|_| EmbeddedSignerError::InvalidPrivateKey)?;
        let public_key = identity.public_key_bytes();
        if public_key != config.public_key_bytes || hex::encode(public_key) != config.public_key_hex
        {
            return Err(EmbeddedSignerError::PublicKeyMismatch);
        }
        let did = identity
            .did()
            .map_err(|_| EmbeddedSignerError::InvalidDid)?
            .to_string();
        Ok(Self { did, config })
    }

    /// The identity derived from this capability's verified keypair.
    pub fn did(&self) -> &str {
        &self.did
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn config(&self) -> SigningConfig {
        self.config.clone()
    }
}

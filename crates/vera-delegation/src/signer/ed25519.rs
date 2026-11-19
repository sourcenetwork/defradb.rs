use crypto::{create_did_key, KeyType};
use defra_core::thread_bounds::MaybeBoxFuture;
use ed25519_dalek::{Signer as _, SigningKey};

use super::{DelegationAlgorithm, DelegationSigner, SignerError};

/// An Ed25519 key this process holds.
pub struct Ed25519Signer {
    key: SigningKey,
    did: String,
}

impl Ed25519Signer {
    /// A signer for `private_key`: the 32-byte seed, or the 64 bytes of the
    /// seed followed by its public key, which is how DefraDB keeps an
    /// Ed25519 identity. A public half that is not the seed's is refused.
    pub fn from_slice(private_key: &[u8]) -> Result<Self, SignerError> {
        let key = if let Ok(seed) = <[u8; 32]>::try_from(private_key) {
            SigningKey::from_bytes(&seed)
        } else if let Ok(pair) = <[u8; 64]>::try_from(private_key) {
            SigningKey::from_keypair_bytes(&pair)?
        } else {
            return Err(SignerError::KeyLength {
                algorithm: DelegationAlgorithm::EdDsa,
                length: private_key.len(),
            });
        };
        let did = create_did_key(KeyType::Ed25519, key.verifying_key().as_bytes())
            .map_err(|error| SignerError::Failed(error.to_string()))?;
        Ok(Self { key, did })
    }
}

impl DelegationSigner for Ed25519Signer {
    fn did(&self) -> &str {
        &self.did
    }

    fn algorithm(&self) -> DelegationAlgorithm {
        DelegationAlgorithm::EdDsa
    }

    fn sign<'a>(
        &'a self,
        signing_input: &'a [u8],
    ) -> MaybeBoxFuture<'a, Result<Vec<u8>, SignerError>> {
        let signature = self.key.sign(signing_input).to_bytes().to_vec();
        Box::pin(std::future::ready(Ok(signature)))
    }
}

use crypto::{create_did_key, KeyType};
use defra_core::thread_bounds::MaybeBoxFuture;
use p256::ecdsa::signature::hazmat::PrehashSigner;
use p256::ecdsa::{Signature, SigningKey};
use sha2::{Digest, Sha256};

use super::{DelegationAlgorithm, DelegationSigner, SignerError};

/// A P-256 key this process holds.
pub struct P256Signer {
    key: SigningKey,
    did: String,
}

impl P256Signer {
    /// A signer for the 32-byte private key `private_key`. Its DID is the
    /// one DefraDB derives, from the uncompressed point; the provider sends
    /// vera.rs the compressed spelling, the only one it accepts.
    pub fn from_slice(private_key: &[u8]) -> Result<Self, SignerError> {
        let key = SigningKey::from_slice(private_key)?;
        let point = key.verifying_key().to_sec1_point(false);
        let did = create_did_key(KeyType::Secp256r1, point.as_bytes())
            .map_err(|error| SignerError::Failed(error.to_string()))?;
        Ok(Self { key, did })
    }
}

impl DelegationSigner for P256Signer {
    fn did(&self) -> &str {
        &self.did
    }

    fn algorithm(&self) -> DelegationAlgorithm {
        DelegationAlgorithm::Es256
    }

    fn sign<'a>(
        &'a self,
        signing_input: &'a [u8],
    ) -> MaybeBoxFuture<'a, Result<Vec<u8>, SignerError>> {
        let digest = Sha256::digest(signing_input);
        // P-256 signing does not pick the lower `s`; vera.rs accepts no other.
        let signature = PrehashSigner::<Signature>::sign_prehash(&self.key, digest.as_ref())
            .map(|signature| signature.normalize_s().to_bytes().to_vec())
            .map_err(SignerError::from);
        Box::pin(std::future::ready(signature))
    }
}

use defra_core::thread_bounds::MaybeBoxFuture;
use sha2::{Digest, Sha256};
use vera_client::Secp256k1SigningKey as SigningKey;

use super::{DelegationAlgorithm, DelegationSigner, SignerError};
use crate::vera_rs::bearer;

/// A secp256k1 key this process holds.
pub struct Secp256k1Signer {
    key: SigningKey,
    did: String,
}

impl Secp256k1Signer {
    /// A signer for the 32-byte private key `private_key`.
    pub fn from_slice(private_key: &[u8]) -> Result<Self, SignerError> {
        let key = SigningKey::from_slice(private_key)?;
        let did = bearer::did_from_key(key.verifying_key(), false);
        Ok(Self { key, did })
    }

    /// The compressed public key in lowercase hexadecimal: the form a vera.rs
    /// operator policy lists a secp256k1 key in.
    pub fn operator_key(&self) -> String {
        hex::encode(self.key.verifying_key().to_sec1_point(true).as_bytes())
    }
}

impl DelegationSigner for Secp256k1Signer {
    fn did(&self) -> &str {
        &self.did
    }

    fn algorithm(&self) -> DelegationAlgorithm {
        DelegationAlgorithm::Es256k
    }

    fn sign<'a>(
        &'a self,
        signing_input: &'a [u8],
    ) -> MaybeBoxFuture<'a, Result<Vec<u8>, SignerError>> {
        let digest = Sha256::digest(signing_input);
        let (signature, _) = self.key.sign_prehash_recoverable(digest.as_ref());
        Box::pin(std::future::ready(Ok(signature.to_bytes().to_vec())))
    }
}

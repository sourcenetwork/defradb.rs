//! Which signatures leave the node.
//!
//! vera.rs verifies a delegation against the key its issuer DID names and
//! refuses a signature that is not 64 bytes, is not two scalars of the curve
//! order, has `s` in the upper half of that order, or does not verify. The
//! provider applies the same tests to what a signer returns, so a token
//! vera.rs would refuse is refused here, with the reason, and nothing vera.rs
//! would accept is (`proofs/lean/VeraDelegationSigner.lean`).

use crypto::{parse_did_key, KeyType};
use k256::ecdsa::signature::hazmat::PrehashVerifier as _;
use k256::ecdsa::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

use super::{DelegationAlgorithm, DelegationSigner, SignerError};

/// The key `signer`'s DID names, which its signatures must verify under.
pub(in crate::vera_rs) fn actor_key(
    signer: &dyn DelegationSigner,
) -> Result<VerifyingKey, SignerError> {
    let DelegationAlgorithm::Es256k = signer.algorithm();
    let unusable = |reason: String| SignerError::Did {
        did: signer.did().to_owned(),
        algorithm: signer.algorithm(),
        reason,
    };
    match parse_did_key(signer.did()) {
        Ok((KeyType::Secp256k1, key)) => VerifyingKey::from_sec1_bytes(&key)
            .map_err(|_| unusable("its key is not a secp256k1 point".into())),
        Ok((other, _)) => Err(unusable(format!("it names a {other:?} key"))),
        Err(error) => Err(unusable(error.to_string())),
    }
}

/// Accepts `signature` over `signing_input` exactly when vera.rs would.
pub(in crate::vera_rs) fn admit(
    signer: &dyn DelegationSigner,
    key: &VerifyingKey,
    signing_input: &[u8],
    signature: &[u8],
) -> Result<(), SignerError> {
    let DelegationAlgorithm::Es256k = signer.algorithm();
    let did = || signer.did().to_owned();
    if signature.len() != 64 {
        return Err(SignerError::Length {
            did: did(),
            algorithm: signer.algorithm(),
            length: signature.len(),
        });
    }
    let signature =
        Signature::from_slice(signature).map_err(|_| SignerError::Scalar { did: did() })?;
    if signature.normalize_s() != signature {
        return Err(SignerError::HighS { did: did() });
    }
    key.verify_prehash(&Sha256::digest(signing_input), &signature)
        .map_err(|_| SignerError::Unverified { did: did() })
}

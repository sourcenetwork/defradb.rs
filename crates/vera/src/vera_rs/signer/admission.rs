//! Which signatures leave the node.
//!
//! vera.rs verifies a delegation against the key its issuer DID names. For an
//! ECDSA key it refuses a signature that is not 64 bytes, is not two scalars
//! of the curve order, has `s` in the upper half of that order, or does not
//! verify. For an Ed25519 key it refuses one that is not 64 bytes or fails
//! strict verification. The provider applies the same tests to what a signer
//! returns, so a token vera.rs would refuse is refused here, with the reason,
//! and nothing vera.rs would accept is
//! (`proofs/lean/VeraDelegationSigner.lean`).

use crypto::{create_did_key, parse_did_key, KeyType};
use k256::ecdsa::signature::hazmat::PrehashVerifier as _;
use sha2::{Digest, Sha256};

use super::{DelegationAlgorithm, DelegationSigner, SignerError};
use crate::vera_rs::bearer;

/// The key a signer's DID names, which its signatures must verify under.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::vera_rs) enum ActorKey {
    Secp256k1(k256::ecdsa::VerifyingKey),
    Ed25519(ed25519_dalek::VerifyingKey),
    P256(p256::ecdsa::VerifyingKey),
}

impl ActorKey {
    /// Whether `issuer` is a spelling of this key that vera.rs accepts as a
    /// token's issuer: either point encoding of a secp256k1 key, and the one
    /// canonical DID of an Ed25519 or P-256 key.
    pub(in crate::vera_rs) fn names(&self, issuer: &str) -> bool {
        match self {
            Self::Secp256k1(key) => {
                issuer == bearer::did_from_key(key, true)
                    || issuer == bearer::did_from_key(key, false)
            }
            Self::Ed25519(key) => {
                create_did_key(KeyType::Ed25519, key.as_bytes()).is_ok_and(|did| did == issuer)
            }
            Self::P256(key) => {
                create_did_key(KeyType::Secp256r1, key.to_sec1_point(true).as_bytes())
                    .is_ok_and(|did| did == issuer)
            }
        }
    }
}

/// The key `signer`'s DID names, of the type its algorithm signs with.
pub(in crate::vera_rs) fn actor_key(
    signer: &dyn DelegationSigner,
) -> Result<ActorKey, SignerError> {
    let unusable = |reason: String| SignerError::Did {
        did: signer.did().to_owned(),
        algorithm: signer.algorithm(),
        reason,
    };
    match (signer.algorithm(), parse_did_key(signer.did())) {
        (DelegationAlgorithm::Es256k, Ok((KeyType::Secp256k1, key))) => {
            k256::ecdsa::VerifyingKey::from_sec1_bytes(&key)
                .map(ActorKey::Secp256k1)
                .map_err(|_| unusable("its key is not a secp256k1 point".into()))
        }
        (DelegationAlgorithm::Es256, Ok((KeyType::Secp256r1, key))) => {
            p256::ecdsa::VerifyingKey::from_sec1_bytes(&key)
                .map(ActorKey::P256)
                .map_err(|_| unusable("its key is not a P-256 point".into()))
        }
        (DelegationAlgorithm::EdDsa, Ok((KeyType::Ed25519, key))) => {
            let key = <[u8; 32]>::try_from(key.as_slice())
                .ok()
                .and_then(|bytes| ed25519_dalek::VerifyingKey::from_bytes(&bytes).ok())
                .ok_or_else(|| unusable("its key is not an Ed25519 point".into()))?;
            // vera.rs refuses a small-order key, under which a signature
            // verifies for more than one message.
            if key.is_weak() {
                return Err(unusable("its key is a small-order Ed25519 point".into()));
            }
            Ok(ActorKey::Ed25519(key))
        }
        (_, Ok((other, _))) => Err(unusable(format!("it names a {other:?} key"))),
        (_, Err(error)) => Err(unusable(error.to_string())),
    }
}

/// Accepts `signature` over `signing_input` exactly when vera.rs would.
pub(in crate::vera_rs) fn admit(
    signer: &dyn DelegationSigner,
    key: &ActorKey,
    signing_input: &[u8],
    signature: &[u8],
) -> Result<(), SignerError> {
    let did = || signer.did().to_owned();
    if signature.len() != 64 {
        return Err(SignerError::Length {
            did: did(),
            algorithm: signer.algorithm(),
            length: signature.len(),
        });
    }
    let verified = match key {
        ActorKey::Secp256k1(key) => {
            let signature = k256::ecdsa::Signature::from_slice(signature)
                .map_err(|_| SignerError::Scalar { did: did() })?;
            if signature.normalize_s() != signature {
                return Err(SignerError::HighS { did: did() });
            }
            key.verify_prehash(&Sha256::digest(signing_input), &signature)
                .is_ok()
        }
        ActorKey::P256(key) => {
            let signature = p256::ecdsa::Signature::from_slice(signature)
                .map_err(|_| SignerError::Scalar { did: did() })?;
            if signature.normalize_s() != signature {
                return Err(SignerError::HighS { did: did() });
            }
            key.verify_prehash(&Sha256::digest(signing_input), &signature)
                .is_ok()
        }
        ActorKey::Ed25519(key) => ed25519_dalek::Signature::from_slice(signature)
            .is_ok_and(|signature| key.verify_strict(signing_input, &signature).is_ok()),
    };
    if verified {
        Ok(())
    } else {
        Err(SignerError::Unverified { did: did() })
    }
}

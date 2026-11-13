//! The Ed25519 and P-256 signers, and what the provider accepts from them.

use std::borrow::Cow;

use super::admission::{actor_key, admit};
use super::*;
use crate::vera_rs::did_spelling;

const INPUT: &[u8] = b"header.payload";

#[nuclei::test]
async fn the_ed25519_signer_signs_under_the_one_did_of_its_key() {
    let signer = Ed25519Signer::from_slice(&[7; 32]).expect("valid key");
    assert_eq!(signer.algorithm().name(), "EdDSA");
    assert!(signer.did().starts_with("did:key:z6Mk"));
    for spelling in [
        did_spelling::defra(signer.did()),
        did_spelling::wire(signer.did()),
    ] {
        assert!(matches!(spelling, Cow::Borrowed(_)));
    }

    let key = actor_key(&signer).expect("Ed25519 actor");
    let signature = signer.sign(INPUT).await.expect("signature");
    admit(&signer, &key, INPUT, &signature).expect("admitted");
    assert!(matches!(
        admit(&signer, &key, b"header.other", &signature),
        Err(SignerError::Unverified { .. })
    ));
    assert!(matches!(
        admit(&signer, &key, INPUT, &signature[..63]),
        Err(SignerError::Length { length: 63, .. })
    ));
    let other = Ed25519Signer::from_slice(&[8; 32]).expect("valid key");
    let foreign = other.sign(INPUT).await.expect("signature");
    assert!(matches!(
        admit(&signer, &key, INPUT, &foreign),
        Err(SignerError::Unverified { .. })
    ));

    for unusable in [&[][..], &[7; 31], &[7; 33], &[7; 65]] {
        assert!(matches!(
            Ed25519Signer::from_slice(unusable),
            Err(SignerError::KeyLength { .. })
        ));
    }

    // The seed with its public key is the same signer; with another key's
    // public half it is refused.
    let public = ed25519_dalek::SigningKey::from_bytes(&[7; 32]).verifying_key();
    let pair = [&[7; 32][..], public.as_bytes()].concat();
    let paired = Ed25519Signer::from_slice(&pair).expect("seed and public key");
    assert_eq!(paired.did(), signer.did());
    let other = ed25519_dalek::SigningKey::from_bytes(&[8; 32]).verifying_key();
    let mismatched = [&[7; 32][..], other.as_bytes()].concat();
    assert!(matches!(
        Ed25519Signer::from_slice(&mismatched),
        Err(SignerError::Key(_))
    ));
}

#[nuclei::test]
async fn the_p256_signer_signs_low_s_and_is_named_in_two_spellings() {
    let signer = P256Signer::from_slice(&[7; 32]).expect("valid key");
    assert_eq!(signer.algorithm().name(), "ES256");
    // DefraDB's spelling is the uncompressed point; vera.rs takes the other.
    assert!(matches!(
        did_spelling::defra(signer.did()),
        Cow::Borrowed(_)
    ));
    let wire = did_spelling::wire(signer.did()).into_owned();
    assert!(wire.starts_with("did:key:zDn"));
    assert_ne!(wire, signer.did());
    assert_eq!(did_spelling::defra(&wire), signer.did());
    assert!(did_spelling::same_actor(&wire, signer.did()));

    let key = actor_key(&signer).expect("P-256 actor");
    assert!(key.names(&wire));
    assert!(!key.names(signer.did()));
    let signature = signer.sign(INPUT).await.expect("signature");
    admit(&signer, &key, INPUT, &signature).expect("admitted");

    let parsed = ::p256::ecdsa::Signature::from_slice(&signature).expect("signature");
    let high = ::p256::ecdsa::Signature::from_scalars(*parsed.r(), -*parsed.s())
        .expect("negated s is a scalar")
        .to_bytes();
    assert!(matches!(
        admit(&signer, &key, INPUT, &high),
        Err(SignerError::HighS { .. })
    ));
    assert!(matches!(
        admit(&signer, &key, INPUT, &[0; 64]),
        Err(SignerError::Scalar { .. })
    ));
    assert!(matches!(
        admit(&signer, &key, b"header.other", &signature),
        Err(SignerError::Unverified { .. })
    ));

    for unusable in [&[][..], &[0; 32], &[0xff; 32], &[7; 33]] {
        assert!(P256Signer::from_slice(unusable).is_err());
    }
}

#[nuclei::test]
async fn a_der_signature_becomes_the_low_s_form_vera_rs_accepts() {
    let signer = P256Signer::from_slice(&[7; 32]).expect("valid key");
    let key = actor_key(&signer).expect("P-256 actor");
    let signature = signer.sign(INPUT).await.expect("signature");
    let parsed = ::p256::ecdsa::Signature::from_slice(&signature).expect("signature");
    let high = ::p256::ecdsa::Signature::from_scalars(*parsed.r(), -*parsed.s())
        .expect("negated s is a scalar");

    // A key store may return either `s`; both become the one accepted form.
    for der in [parsed.to_der(), high.to_der()] {
        let jws = signature_from_der(DelegationAlgorithm::Es256, der.as_bytes()).expect("DER");
        assert_eq!(jws, signature);
        admit(&signer, &key, INPUT, &jws).expect("admitted");
    }

    let secp256k1 = Secp256k1Signer::from_slice(&[7; 32]).expect("valid key");
    let raw = secp256k1.sign(INPUT).await.expect("signature");
    let der = k256::ecdsa::Signature::from_slice(&raw)
        .expect("signature")
        .to_der();
    assert_eq!(
        signature_from_der(DelegationAlgorithm::Es256k, der.as_bytes()).expect("DER"),
        raw
    );

    for (algorithm, bytes) in [
        (DelegationAlgorithm::Es256, &signature[..]),
        (DelegationAlgorithm::Es256k, &[][..]),
        (DelegationAlgorithm::EdDsa, der.as_bytes()),
    ] {
        assert!(matches!(
            signature_from_der(algorithm, bytes),
            Err(SignerError::Der { .. })
        ));
    }
}

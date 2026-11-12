//! The built-in secp256k1 signer, and what the provider accepts from any
//! signer.

use std::borrow::Cow;
use std::sync::Arc;

use k256::ecdsa::Signature;
use regolith::sync::SegQueue;
use vera_client::Secp256k1SigningKey as SigningKey;

use super::admission::{actor_key, admit};
use super::*;
use crate::vera_rs::{bearer, did_spelling};

/// What a [`Recording`] signer answers a signing request with.
#[derive(Clone, Copy, Debug)]
pub(in crate::vera_rs) enum Returns {
    /// The signature of its key over the signing input.
    Genuine,
    /// That signature without its last byte.
    Truncated,
    /// That signature with one byte too many.
    Padded,
    /// That signature with `s` replaced by its negation, in the upper half.
    HighS,
    /// The signature of another key over the signing input.
    OtherKey,
    /// 64 zero bytes: neither `r` nor `s` is a scalar of the curve order.
    Zero,
    /// No signature: the signer reports a failure.
    Failure,
}

/// A signer for the key `[7; 32]` that records every request and what it
/// answered.
pub(in crate::vera_rs) struct Recording {
    key: Secp256k1Signer,
    returns: Returns,
    pub(in crate::vera_rs) signed: SegQueue<(Vec<u8>, Vec<u8>)>,
}

impl Recording {
    pub(in crate::vera_rs) fn returning(returns: Returns) -> Arc<Self> {
        Arc::new(Self {
            key: Secp256k1Signer::from_slice(&[7; 32]).expect("valid key"),
            returns,
            signed: SegQueue::new(),
        })
    }

    async fn answer(&self, signing_input: &[u8]) -> Result<Vec<u8>, SignerError> {
        let genuine = self.key.sign(signing_input).await?;
        Ok(match self.returns {
            Returns::Genuine => genuine,
            Returns::Truncated => genuine[..63].to_vec(),
            Returns::Padded => [genuine.as_slice(), &[0]].concat(),
            Returns::HighS => {
                let signature = Signature::from_slice(&genuine).expect("signature");
                Signature::from_scalars(*signature.r(), -*signature.s())
                    .expect("negated s is a scalar")
                    .to_bytes()
                    .to_vec()
            }
            Returns::OtherKey => {
                let other = Secp256k1Signer::from_slice(&[8; 32]).expect("valid key");
                other.sign(signing_input).await?
            }
            Returns::Zero => vec![0; 64],
            Returns::Failure => return Err(SignerError::Failed("the key is locked".into())),
        })
    }
}

impl DelegationSigner for Recording {
    fn did(&self) -> &str {
        self.key.did()
    }

    fn algorithm(&self) -> DelegationAlgorithm {
        self.key.algorithm()
    }

    fn sign<'a>(
        &'a self,
        signing_input: &'a [u8],
    ) -> MaybeBoxFuture<'a, Result<Vec<u8>, SignerError>> {
        Box::pin(async move {
            let answer = self.answer(signing_input).await;
            self.signed.push((
                signing_input.to_vec(),
                answer.as_ref().cloned().unwrap_or_default(),
            ));
            answer
        })
    }
}

/// A signer that only names a DID.
struct Named(&'static str);

impl DelegationSigner for Named {
    fn did(&self) -> &str {
        self.0
    }

    fn algorithm(&self) -> DelegationAlgorithm {
        DelegationAlgorithm::Es256k
    }

    fn sign<'a>(&'a self, _: &'a [u8]) -> MaybeBoxFuture<'a, Result<Vec<u8>, SignerError>> {
        Box::pin(std::future::ready(Err(SignerError::Failed(
            "this signer holds no key".into(),
        ))))
    }
}

#[test]
fn the_secp256k1_signer_names_its_actor_in_the_uncompressed_spelling() {
    let key = SigningKey::from_slice(&[7; 32]).expect("valid key");
    let signer = Secp256k1Signer::from_slice(&[7; 32]).expect("valid key");
    assert_eq!(signer.did(), bearer::did_from_signing_key(&key, false));
    assert!(matches!(
        did_spelling::defra(signer.did()),
        Cow::Borrowed(_)
    ));
    assert_eq!(signer.algorithm().name(), "ES256K");

    for unusable in [&[][..], &[0; 32], &[0xff; 32], &[7; 33]] {
        let refused = Secp256k1Signer::from_slice(unusable)
            .err()
            .expect("refused");
        let library = SigningKey::from_slice(unusable).expect_err("refused");
        assert_eq!(refused.to_string(), library.to_string());
    }
}

#[nuclei::test]
async fn a_signature_is_admitted_exactly_when_vera_rs_would_verify_it() {
    let input = b"header.payload";
    for returns in [
        Returns::Genuine,
        Returns::Truncated,
        Returns::Padded,
        Returns::Zero,
        Returns::HighS,
        Returns::OtherKey,
    ] {
        let signer = Recording::returning(returns);
        let did = signer.did().to_owned();
        let key = actor_key(signer.as_ref()).expect("secp256k1 actor");
        let signature = signer.sign(input).await.expect("signature");
        let outcome = admit(signer.as_ref(), &key, input, &signature);
        let said = outcome.as_ref().map_err(ToString::to_string);
        match (returns, &outcome) {
            (Returns::Genuine, Ok(())) => {}
            (Returns::Truncated, Err(SignerError::Length { length: 63, .. }))
            | (Returns::Padded, Err(SignerError::Length { length: 65, .. })) => {
                let said = said.unwrap_err();
                assert!(said.contains(&format!("{}-byte signature", signature.len())));
                assert!(said.contains("an ES256K signature is 64 bytes"));
                assert!(said.contains(&did));
            }
            (Returns::Zero, Err(SignerError::Scalar { .. }))
            | (Returns::HighS, Err(SignerError::HighS { .. }))
            | (Returns::OtherKey, Err(SignerError::Unverified { .. })) => {
                assert!(said.unwrap_err().contains(&did));
            }
            other => panic!("unexpected outcome {other:?}"),
        }
    }

    let signer = Recording::returning(Returns::Genuine);
    let key = actor_key(signer.as_ref()).expect("secp256k1 actor");
    let signature = signer.sign(input).await.expect("signature");
    assert!(matches!(
        admit(signer.as_ref(), &key, b"header.other", &signature),
        Err(SignerError::Unverified { .. })
    ));
}

#[test]
fn a_signer_must_name_a_secp256k1_key() {
    let key = SigningKey::from_slice(&[7; 32]).expect("valid key");
    let uncompressed: &'static str = bearer::did_from_signing_key(&key, false).leak();
    let compressed: &'static str = bearer::did_from_signing_key(&key, true).leak();
    assert_eq!(
        actor_key(&Named(compressed)).expect("compressed spelling"),
        actor_key(&Named(uncompressed)).expect("uncompressed spelling"),
    );

    let off_curve: &'static str = crypto::create_did_key(crypto::KeyType::Secp256k1, &[0xff; 33])
        .expect("did:key")
        .leak();
    for (did, reason) in [
        (
            "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp",
            "it names a Ed25519 key",
        ),
        ("did:web:example.com", "missing 'did:key:' prefix"),
        (off_curve, "its key is not a secp256k1 point"),
    ] {
        let refused = actor_key(&Named(did)).expect_err("not a secp256k1 key");
        assert!(matches!(refused, SignerError::Did { .. }));
        let said = refused.to_string();
        assert!(said.starts_with(&format!("{did} is not a secp256k1 did:key")));
        assert!(said.contains("an ES256K delegation needs"));
        assert!(said.contains(reason), "{said}");
    }
}

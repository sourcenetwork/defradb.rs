//! A signer that answers as a test tells it to, and records what it signed.

use std::sync::Arc;

use defra_core::thread_bounds::MaybeBoxFuture;
use k256::ecdsa::Signature;
use regolith::sync::SegQueue;

use crate::signer::{DelegationAlgorithm, DelegationSigner, Secp256k1Signer, SignerError};

/// What a [`Recording`] signer answers a signing request with.
#[derive(Clone, Copy, Debug)]
pub enum Returns {
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

/// A signer for the secp256k1 key `[7; 32]` that records every request and
/// what it answered.
pub struct Recording {
    key: Secp256k1Signer,
    returns: Returns,
    /// Each signing input, with the signature returned for it (empty on a
    /// failure).
    pub signed: SegQueue<(Vec<u8>, Vec<u8>)>,
}

impl Recording {
    pub fn returning(returns: Returns) -> Arc<Self> {
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

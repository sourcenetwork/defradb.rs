use std::sync::Arc;

use defra_core::signing::{RemoteSigner, SignFailure, SigningAuthorization};
use defra_core::thread_bounds::MaybeBoxFuture;
use nuclei::sync::{mpsc, oneshot};

pub(crate) struct Wave {
    pub(crate) payloads: Vec<Arc<[u8]>>,
    pub(crate) answer: oneshot::Sender<Vec<Vec<u8>>>,
}

/// One signer belongs to one connection and never consults the DID registry.
pub(crate) struct SessionSigner {
    public: Arc<dyn crypto::PublicKey>,
    requests: mpsc::Sender<Wave>,
}

impl SessionSigner {
    pub(crate) fn new(public_key_hex: &str) -> defra_core::Result<(Self, mpsc::Receiver<Wave>)> {
        let public = crypto::public_key_from_string(crypto::KeyType::Ed25519, public_key_hex)?;
        let (requests, receiver) = mpsc::channel(1);
        Ok((
            Self {
                public: Arc::from(public),
                requests,
            },
            receiver,
        ))
    }
}

impl RemoteSigner for SessionSigner {
    fn sign_sync(&self, _: &[u8], _: Option<&SigningAuthorization>) -> Result<Vec<u8>, String> {
        Err("connection signing requires asynchronous preparation".to_owned())
    }

    fn sign_batch(
        &self,
        payloads: Vec<Arc<[u8]>>,
        _: Option<SigningAuthorization>,
    ) -> MaybeBoxFuture<'static, Vec<Result<Vec<u8>, SignFailure>>> {
        let public = self.public.clone();
        let requests = self.requests.clone();
        Box::pin(async move {
            let count = payloads.len();
            let (answer, received) = oneshot::channel();
            if requests
                .send(Wave {
                    payloads: payloads.clone(),
                    answer,
                })
                .await
                .is_err()
            {
                return vec![Err(SignFailure::refused("signing connection closed")); count];
            }
            let signatures = match received.await {
                Ok(signatures) if signatures.len() == count => signatures,
                _ => return vec![Err(SignFailure::refused("invalid signing response")); count],
            };
            payloads
                .iter()
                .zip(signatures)
                .map(
                    |(payload, signature)| match public.verify(payload, &signature) {
                        Ok(true) => Ok(signature),
                        _ => Err(SignFailure::refused("client signature did not verify")),
                    },
                )
                .collect()
        })
    }
}

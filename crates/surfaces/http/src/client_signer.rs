use std::sync::Arc;

use defra_core::signing::{RemoteSigner, SignFailure, SigningAuthorization};
use defra_core::thread_bounds::MaybeBoxFuture;
use nuclei::sync::{mpsc, oneshot};

use crate::client_signing_waves::SigningWaveError;

/// One wave the session asks the client to sign. The session's
/// [`SigningWaves`](crate::client_signing_waves::SigningWaves) verifies the
/// answer before it is sent back.
pub(crate) struct Wave {
    pub(crate) payloads: Vec<Arc<[u8]>>,
    pub(crate) answer: oneshot::Sender<Result<Vec<Vec<u8>>, SigningWaveError>>,
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
    fn uses_caller_key(&self, caller: &str) -> bool {
        self.public.did().is_ok_and(|did| did == caller)
    }

    fn sign_sync(&self, _: &[u8], _: Option<&SigningAuthorization>) -> Result<Vec<u8>, String> {
        Err("connection signing requires asynchronous preparation".to_owned())
    }

    fn sign_batch(
        &self,
        payloads: Vec<Arc<[u8]>>,
        _: Option<SigningAuthorization>,
    ) -> MaybeBoxFuture<'static, Vec<Result<Vec<u8>, SignFailure>>> {
        let requests = self.requests.clone();
        Box::pin(async move {
            let count = payloads.len();
            let (answer, received) = oneshot::channel();
            if requests.send(Wave { payloads, answer }).await.is_err() {
                return vec![Err(SignFailure::refused("signing connection closed")); count];
            }
            match received.await {
                Ok(Ok(signatures)) if signatures.len() == count => {
                    signatures.into_iter().map(Ok).collect()
                }
                Ok(Err(refused)) => vec![Err(SignFailure::refused(refused.to_string())); count],
                _ => vec![Err(SignFailure::refused("invalid signing response")); count],
            }
        })
    }
}

//! Remote signatures as an [`inputs::Source`]: every signature of a wave
//! that one signer makes goes out in one `sign_batch` call.
//!
//! Nothing is cached across requests: the signed bytes carry random nonces,
//! so no two requests sign the same payload, and the request's own
//! prepared-input memo already keeps each signature for its re-runs.
//! Signing the same bytes twice is safe, so a wave every signature of which
//! failed on the way to the signer is sent again.

use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use defra_core::signing::{RemoteSigner, SignFailure, SigningAuthorization};
use defra_core::thread_bounds::MaybeBoxFuture;

/// A remote signer and the authorization its requests carry. Two are equal
/// when they are the same signer object under the same authorization.
#[derive(Clone)]
pub(crate) struct Signer {
    pub(crate) remote: Arc<dyn RemoteSigner>,
    pub(crate) authorization: Option<SigningAuthorization>,
}

impl PartialEq for Signer {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.remote), Arc::as_ptr(&other.remote))
            && self.authorization == other.authorization
    }
}

impl Eq for Signer {}

impl Hash for Signer {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        (Arc::as_ptr(&self.remote) as *const () as usize).hash(state);
        self.authorization.hash(state);
    }
}

/// One signature to make: the exact bytes, by one signer.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct Signature {
    pub(crate) signer: Signer,
    pub(crate) data: Arc<[u8]>,
}

pub(crate) struct RemoteSignatures;

/// A wave is one call per signer; a signer that signs concurrently gets the
/// whole wave at once.
pub(super) fn config() -> inputs::Config {
    inputs::Config {
        // Past the Orbis channel's own 60 s request timeout.
        deadline: Duration::from_secs(90),
        attempts: 3,
        max_batch: 1024,
        max_in_flight: 16,
        cache_bytes: 0,
        ..inputs::Config::default()
    }
}

impl inputs::Source for RemoteSignatures {
    type Key = Signature;
    type Group = Signer;
    type Value = Arc<[u8]>;
    type Error = SignFailure;

    fn name(&self) -> &'static str {
        "signatures"
    }

    fn group(&self, key: &Signature) -> Signer {
        key.signer.clone()
    }

    fn fetch<'a>(
        &'a self,
        signer: &'a Signer,
        keys: &'a [Signature],
    ) -> MaybeBoxFuture<'a, inputs::Answers<Arc<[u8]>, SignFailure>> {
        let payloads = keys.iter().map(|key| Arc::clone(&key.data)).collect();
        let wave = signer
            .remote
            .sign_batch(payloads, signer.authorization.clone());
        Box::pin(async move {
            let signatures = wave.await;
            // Nothing reached the signer: the wave as a whole may go again.
            if let Some(failure) = signatures
                .iter()
                .map(|signature| signature.as_ref().err().filter(|failure| failure.transient))
                .collect::<Option<Vec<_>>>()
                .and_then(|failures| failures.first().map(|failure| (*failure).clone()))
            {
                return Err(failure);
            }
            Ok(signatures
                .into_iter()
                .map(|signature| signature.map(Arc::from))
                .collect())
        })
    }

    fn retryable(&self, error: &SignFailure) -> bool {
        error.transient
    }

    fn cache_bytes(&self, _key: &Signature, _value: &Arc<[u8]>) -> Option<usize> {
        None
    }
}

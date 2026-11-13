//! Keys fetched from peers, as an [`inputs::Source`]: one wire request per
//! batch of CIDs that share an explicit-replay capability, with one
//! ephemeral key, fanned out over every transport.
//!
//! A peer authorizes the requesting node (and the capability), never the
//! user behind the request, so one peer answer serves every caller asking
//! for the same CID under the same capability: that pair is the key
//! single-flight and the cache use. A denial is an error and never cached.

use std::sync::Arc;

use defra_core::thread_bounds::MaybeBoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use identity::Did;
use kovan::AtomOption;
use rapidhash::{HashMapExt, RapidHashMap};
use zeroize::Zeroizing;

use crate::error::{Error, Result};
use crate::store::StoredKey;
use crate::tier::{verified_key, KeyTier};
use crate::transport::{EncodedFetchRequest, KeyTransport, TransportReplyStream};
use crate::types::EncryptionCid;
use crate::wire::{FetchEncryptionKeyReply, FetchEncryptionKeyRequest};

/// One remote key request: the key's CID and the capability it is asked
/// under.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RemoteKey {
    pub(crate) cid: EncryptionCid,
    pub(crate) capability: Option<Arc<str>>,
}

/// A key a peer released, wiped from memory when the last copy goes.
pub(crate) type RemoteDek = Arc<Zeroizing<[u8; 32]>>;

pub(crate) struct RemoteKeys {
    pub(crate) tier: Arc<KeyTier>,
    pub(crate) transports: Vec<Arc<dyn KeyTransport>>,
    /// Node identity sent on cross-peer requests and authenticated by peers.
    pub(crate) node_identity: Did,
    /// Test-only deterministic ephemeral. `None` in prod: fresh per request.
    pub(crate) test_ephemeral: Arc<AtomOption<x25519_dalek::StaticSecret>>,
}

impl inputs::Source for RemoteKeys {
    type Key = RemoteKey;
    type Group = Option<Arc<str>>;
    type Value = RemoteDek;
    type Error = Error;

    fn name(&self) -> &'static str {
        "kms"
    }

    fn group(&self, key: &RemoteKey) -> Option<Arc<str>> {
        key.capability.clone()
    }

    fn fetch<'a>(
        &'a self,
        group: &'a Option<Arc<str>>,
        keys: &'a [RemoteKey],
    ) -> MaybeBoxFuture<'a, inputs::Answers<RemoteDek, Error>> {
        Box::pin(self.fetch_batch(group.as_deref(), keys))
    }

    /// A request no transport could send may go out again; a request that
    /// could not be built cannot.
    fn retryable(&self, error: &Error) -> bool {
        !matches!(
            error,
            Error::WireEncode(_) | Error::Crypto(_) | Error::Unsupported(_) | Error::NoTransport
        )
    }

    fn cache_bytes(&self, key: &RemoteKey, _value: &RemoteDek) -> Option<usize> {
        Some(
            32 + 64
                + key
                    .capability
                    .as_ref()
                    .map_or(0, |capability| capability.len()),
        )
    }
}

/// The next reply on `stream`, with the stream to read the one after.
async fn next_reply(
    mut stream: TransportReplyStream,
) -> (
    Option<Result<(FetchEncryptionKeyReply, String)>>,
    TransportReplyStream,
) {
    let item = stream.recv().await;
    (item, stream)
}

impl RemoteKeys {
    fn ephemeral(&self) -> Result<x25519_dalek::StaticSecret> {
        match self.test_ephemeral.load() {
            Some(secret) => Ok((*secret).clone()),
            None => crypto::generate_x25519().map_err(|error| Error::Crypto(error.to_string())),
        }
    }

    /// One wire request for every key of the batch; answers in their order.
    /// A key no peer released carries the last failure seen.
    async fn fetch_batch(
        &self,
        capability: Option<&str>,
        keys: &[RemoteKey],
    ) -> inputs::Answers<RemoteDek, Error> {
        // Cross-peer DEK access is authorized as the requesting node. Go
        // responders still consume this legacy wire field, while Rust
        // responders bind it to the authenticated transport peer.
        let eph = self.ephemeral()?;
        let our_eph_pub = x25519_dalek::PublicKey::from(&eph).as_bytes().to_vec();
        let req = FetchEncryptionKeyRequest {
            identity: self.node_identity.to_string().into_bytes(),
            links: keys.iter().map(|key| key.cid.to_bytes()).collect(),
            ephemeral_public_key: our_eph_pub.clone(),
            explicit_replay_capability: capability.map(str::to_owned),
        };
        let payload =
            defra_core::cbor::to_vec(&req).map_err(|e| Error::WireEncode(e.to_string()))?;
        let encoded = EncodedFetchRequest {
            payload,
            request_id: uuid::Uuid::new_v4().to_string(),
        };
        let mut wanted = RapidHashMap::with_capacity(keys.len());
        for (index, key) in keys.iter().enumerate() {
            wanted.insert(key.cid, index);
        }

        // Every transport's replies, read as they come, in this request.
        let mut replies = FuturesUnordered::new();
        let mut send_error = None;
        for transport in &self.transports {
            match transport.send_request(encoded.clone()).await {
                Ok(stream) => replies.push(next_reply(stream)),
                Err(error) => send_error = Some(error),
            }
        }
        if replies.is_empty() {
            return Err(send_error.unwrap_or(Error::NoTransport));
        }

        let mut answers: Vec<Option<Result<RemoteDek>>> = vec![None; keys.len()];
        let mut remaining = keys.len();
        let mut denied = None;
        let mut unavailable = None;
        while remaining > 0 {
            let Some((item, stream)) = replies.next().await else {
                break;
            };
            // A closed stream: that transport stopped listening.
            let Some(transport_result) = item else {
                continue;
            };
            replies.push(next_reply(stream));
            let (reply, responder_peer_id) = match transport_result {
                Ok(reply) => reply,
                Err(error @ Error::AccessDenied { .. }) => {
                    denied = Some(error);
                    continue;
                }
                Err(error) => {
                    unavailable = Some(error);
                    continue;
                }
            };
            tracing::debug!(
                responder = %responder_peer_id,
                links = reply.links.len(),
                blocks = reply.blocks.len(),
                "KMS key request: received a reply"
            );
            if reply.links.len() != reply.blocks.len() {
                let error = Error::Crypto(format!(
                    "KMS reply links/blocks length mismatch: {} links, {} blocks",
                    reply.links.len(),
                    reply.blocks.len()
                ));
                tracing::warn!(error = %error, "Rejecting malformed KMS reply");
                unavailable = Some(error);
                continue;
            }
            // AAD binds OUR (requester) ephemeral pubkey and the RESPONDER's
            // peer id, matching the serve side.
            let aad = crate::ecies_envelope::make_associated_data(&our_eph_pub, &responder_peer_id);
            for (cid_bytes, block_env) in reply.links.iter().zip(reply.blocks.iter()) {
                let Ok(cid) = cid::Cid::try_from(cid_bytes.as_slice()) else {
                    tracing::warn!("KMS reply contained malformed CID; skipping");
                    unavailable = Some(Error::Crypto("KMS reply contained malformed CID".into()));
                    continue;
                };
                // A reply for a CID not asked for, or answered already.
                let Some(&index) = wanted.get(&cid) else {
                    continue;
                };
                if answers[index].is_some() {
                    continue;
                }
                match self
                    .accept(cid, &eph, block_env, &reply.ephemeral_public_key, &aad)
                    .await
                {
                    Ok(key) => {
                        answers[index] = Some(Ok(Arc::new(Zeroizing::new(key))));
                        remaining -= 1;
                    }
                    Err(error) => unavailable = Some(error),
                }
            }
        }
        let missing = unavailable.or(denied).unwrap_or(Error::KeyUnavailable);
        Ok(answers
            .into_iter()
            .map(|answer| answer.unwrap_or_else(|| Err(missing.clone())))
            .collect())
    }

    /// Unwraps, verifies and persists one released key.
    async fn accept(
        &self,
        cid: EncryptionCid,
        eph: &x25519_dalek::StaticSecret,
        block_env: &[u8],
        responder_eph: &[u8],
        aad: &[u8],
    ) -> Result<[u8; 32]> {
        let block_bytes =
            crate::ecies_envelope::unwrap_with_private(block_env, eph, responder_eph, aad)
                .map_err(|error| {
                    tracing::warn!(error = %error, cid = %cid, "ECIES unwrap failed on KMS reply");
                    Error::Crypto(format!("unwrap encryption block {cid}: {error}"))
                })?;
        let key = verified_key(&cid, &block_bytes).inspect_err(|error| {
            tracing::warn!(
                error = %error,
                cid = %cid,
                "Rejecting invalid encryption block from KMS peer"
            );
        })?;
        self.tier
            .put(cid, StoredKey { key, block_bytes })
            .await
            .inspect_err(|error| {
                tracing::warn!(
                    error = %error,
                    cid = %cid,
                    "Failed to store verified encryption block from KMS peer"
                );
            })?;
        tracing::debug!(cid = %cid, "KMS key request: DEK unwrapped and stored");
        Ok(key)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "remote_tests.rs"]
mod tests;

//! Top-level KMS service trait.

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;

use rapidhash::{HashMapExt, RapidHashMap};

use crate::context::RequestContext;
use crate::error::{Error, Result};
use crate::results::KeyResults;
use crate::types::{EncryptionCid, KeyScope};
use crate::wire::{FetchEncryptionKeyReply, FetchEncryptionKeyRequest};

/// Identity of an incoming peer established at the transport boundary.
#[derive(Debug, Clone)]
pub struct PeerIdentity {
    /// Transport-level peer id (libp2p `PeerId` stringified, or iroh node id).
    pub peer_id: String,
    /// DID authenticated by the transport's peer-identity protocol. `None`
    /// means the transport could not establish an authorization principal.
    pub authenticated_did: Option<identity::Did>,
    /// Replay delegation verified by the transport against this peer.
    pub explicit_replay_authorization: Option<defra_core::merge::ExplicitReplayAuthorization>,
}

/// Top-level KMS surface. One concrete implementation (`DefraKms`) lives
/// alongside test-only `NoopKms`; future milestones may add `RemoteHttpKms`,
/// `SubscriberKms`, etc.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait KmsService: MaybeSendSync {
    /// Fetch DEKs for the given Encryption-block CIDs. Returns a streaming
    /// aggregator because keys may arrive from peers asynchronously.
    /// `ctx` carries the caller identity (set at HTTP/CLI entry points).
    async fn get_keys(&self, ctx: &RequestContext, cids: &[EncryptionCid]) -> Result<KeyResults>;

    /// One answer per CID, in the order asked: the form a caller that asks
    /// for several keys at once and needs each one's outcome uses. The keys
    /// not held locally go to peers in one request.
    ///
    /// The default drains [`get_keys`](Self::get_keys), whose errors name no
    /// CID: a CID it got no key for carries the last error it saw.
    async fn get_keys_each(
        &self,
        ctx: &RequestContext,
        cids: &[EncryptionCid],
    ) -> Result<Vec<Result<[u8; 32]>>> {
        let mut receiver = self.get_keys(ctx, cids).await?.into_receiver();
        let mut keys = RapidHashMap::with_capacity(cids.len());
        let mut last_error = None;
        while let Some(item) = receiver.recv().await {
            match item {
                Ok((cid, key)) => {
                    keys.insert(cid, key);
                }
                Err(error) => last_error = Some(error),
            }
        }
        Ok(cids
            .iter()
            .map(|cid| {
                keys.get(cid)
                    .copied()
                    .ok_or_else(|| last_error.clone().unwrap_or(Error::KeyUnavailable))
            })
            .collect())
    }

    /// One answer per CID, in the order asked, from the keys this node holds
    /// and may release: no peer is asked. A key held elsewhere is
    /// [`Error::KeyUnavailable`].
    ///
    /// The default holds no keys.
    async fn get_local_keys_each(
        &self,
        _ctx: &RequestContext,
        cids: &[EncryptionCid],
    ) -> Result<Vec<Result<[u8; 32]>>> {
        Ok(cids.iter().map(|_| Err(Error::KeyUnavailable)).collect())
    }

    /// Starts fetching the keys of `cids` this node does not hold, without
    /// waiting for them, so a merge that needs a key later finds it local or
    /// joins the request in flight instead of making another.
    ///
    /// The default asks [`get_keys_each`](Self::get_keys_each) and drops the
    /// answers.
    async fn prefetch_keys(&self, ctx: &RequestContext, cids: &[EncryptionCid]) {
        if let Err(error) = self.get_keys_each(ctx, cids).await {
            tracing::debug!(%error, keys = cids.len(), "key prefetch failed");
        }
    }

    /// Generate a fresh DEK for the given scope, persist it locally, and
    /// return both the CID of the on-disk `Encryption` block AND the plain
    /// key bytes the caller will use to encrypt data. Returning both avoids
    /// a redundant `get_keys` round trip on the write path.
    async fn generate_key(
        &self,
        ctx: &RequestContext,
        scope: KeyScope,
    ) -> Result<(EncryptionCid, [u8; 32])>;

    /// Handle an incoming peer key request. Invoked by `KeyTransport`
    /// adapters (e.g. `PubsubKeyTransport`) when a request arrives on
    /// the wire.
    async fn serve_request(
        &self,
        from: PeerIdentity,
        req: FetchEncryptionKeyRequest,
    ) -> Result<FetchEncryptionKeyReply>;

    /// Set this node's transport-level peer id. Bound into the ECIES AAD on
    /// served replies (per Go's `makeAssociatedData`). Default no-op for
    /// implementations that don't serve over a transport (e.g. `NoopKms`).
    fn set_local_peer_id(&self, _id: String) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn assert_object_safe<T: ?Sized + Send + Sync>() {}

    #[test]
    fn kms_service_is_object_safe() {
        assert_object_safe::<dyn KmsService>();
    }
}

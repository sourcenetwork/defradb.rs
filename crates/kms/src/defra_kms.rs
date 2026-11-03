//! Concrete `KmsService` implementation.
//!
//! Composes a `KeyStore` behind an in-memory key tier, zero or more
//! `KeyTransport`s behind one remote-input service, and one `AccessPolicy`.
//! Keys this node holds are released by the policy; the rest go to peers in
//! one request per batch, single-flighted per CID, and replies are ECIES
//! envelopes addressed to the requester's per-request ephemeral pubkey.

use async_trait::async_trait;
use identity::Did;
use kovan::{Atom, AtomOption};
use std::sync::Arc;
use std::time::Duration;

use crate::context::RequestContext;
use crate::error::{Error, Result};
use crate::policy::AccessPolicy;
use crate::remote::{RemoteKey, RemoteKeys};
use crate::results::KeyResults;
use crate::service::{KmsService, PeerIdentity};
use crate::store::KeyStore;
use crate::tier::KeyTier;
use crate::transport::KeyTransport;
use crate::types::{EncryptionCid, KeyScope, PolicyDecision};
use crate::wire::{FetchEncryptionKeyReply, FetchEncryptionKeyRequest};

/// How [`DefraKms`] reaches peers for keys, and how many keys it keeps in
/// memory.
#[derive(Clone, Debug)]
pub struct KmsInputs {
    /// The network and clock peer requests run on.
    pub runtime: inputs::Runtime,
    /// Deadline, retries, batches and the cache of peer answers. A peer
    /// answer is cached for the requesting node and capability, so its time
    /// to live bounds how long a revocation on the peer takes to be seen.
    pub remote: inputs::Config,
    /// Bytes of verified keys kept in memory in front of the key store.
    pub key_tier_bytes: usize,
}

impl KmsInputs {
    /// Peer requests on the network edge `edge`, with the default bounds.
    pub fn on(edge: &edge::Edge) -> Self {
        Self {
            runtime: inputs::Runtime::edge(edge.clone()),
            ..Self::default()
        }
    }
}

/// Peer requests in the requesting task, with no runtime of their own
/// (`inputs::Runtime::inline`): each request asks its own peers, bounded by
/// the transports' timeouts. A node passes its network edge instead, through
/// [`KmsInputs::on`], which shares requests between callers and times them.
impl Default for KmsInputs {
    fn default() -> Self {
        Self {
            runtime: inputs::Runtime::inline(),
            remote: inputs::Config {
                // Past the transports' own subscriber wait and reply timeout.
                deadline: Duration::from_secs(30),
                attempts: 2,
                max_batch: 512,
                max_in_flight: 16,
                cache_bytes: 1 << 20,
                ttl: Some(Duration::from_secs(60)),
                ..inputs::Config::default()
            },
            key_tier_bytes: 8 << 20,
        }
    }
}

/// Default `KmsService` implementation.
pub struct DefraKms {
    tier: Arc<KeyTier>,
    remote: inputs::Service<RemoteKeys>,
    policy: Arc<dyn AccessPolicy>,
    doc_resolver: Arc<dyn BlockDocIDResolver>,
    /// This node's transport-level peer id, bound into the ECIES AAD of
    /// served replies (Go's `makeAssociatedData`). Empty until the node
    /// wiring sets it from the P2P transport.
    local_peer_id: Atom<String>,
    /// Test-only deterministic ephemeral, shared with the peer requests.
    #[cfg(test)]
    test_ephemeral: Arc<AtomOption<x25519_dalek::StaticSecret>>,
}

impl DefraKms {
    /// Construct a new `DefraKms`. Transports may be empty (single-node).
    /// Peer requests run in the requesting task; a node passes its network
    /// edge through [`with_inputs`](Self::with_inputs).
    pub fn new(
        store: Arc<dyn KeyStore>,
        transports: Vec<Arc<dyn KeyTransport>>,
        policy: Arc<dyn AccessPolicy>,
        doc_resolver: Arc<dyn BlockDocIDResolver>,
        node_identity: Did,
    ) -> Self {
        Self::with_inputs(
            store,
            transports,
            policy,
            doc_resolver,
            node_identity,
            KmsInputs::default(),
        )
    }

    /// [`new`](Self::new), with peer requests and the key tier shaped by
    /// `inputs`.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    pub fn with_inputs(
        store: Arc<dyn KeyStore>,
        transports: Vec<Arc<dyn KeyTransport>>,
        policy: Arc<dyn AccessPolicy>,
        doc_resolver: Arc<dyn BlockDocIDResolver>,
        node_identity: Did,
        inputs: KmsInputs,
    ) -> Self {
        let tier = Arc::new(KeyTier::new(store, inputs.key_tier_bytes));
        let test_ephemeral = Arc::new(AtomOption::none());
        let remote = inputs::Service::new(
            RemoteKeys {
                tier: Arc::clone(&tier),
                transports,
                node_identity,
                test_ephemeral: Arc::clone(&test_ephemeral),
            },
            inputs.remote,
            inputs.runtime,
        );
        Self {
            tier,
            remote,
            policy,
            doc_resolver,
            local_peer_id: Atom::new(String::new()),
            #[cfg(test)]
            test_ephemeral,
        }
    }

    /// The metrics of key requests to peers.
    pub fn remote_stats(&self) -> &inputs::Stats {
        self.remote.stats()
    }

    fn local_peer_id(&self) -> String {
        self.local_peer_id.load_clone()
    }

    fn has_transports(&self) -> bool {
        !self.remote.source().transports.is_empty()
    }

    /// Test hook: inject a deterministic ephemeral so a unit test can
    /// decrypt replies produced by a `FakeTransport`.
    #[cfg(test)]
    pub(crate) fn set_ephemeral_for_test(&self, eph: x25519_dalek::StaticSecret) {
        self.test_ephemeral.store_some(eph);
    }

    /// The answer for `cid` from this node's own store: `None` when the key
    /// is not here, or here before the node knows who may have it.
    async fn local_answer(
        &self,
        actor: Option<&Did>,
        cid: &EncryptionCid,
    ) -> Result<Option<Result<[u8; 32]>>> {
        let Some(stored) = self.tier.get(cid).await? else {
            return Ok(None);
        };
        Ok(match self.local_release_decision(actor, cid, None).await? {
            LocalRelease::Allow => Some(Ok(stored.key)),
            LocalRelease::Deny => Some(Err(Error::AccessDenied {
                reason: "policy denied".into(),
            })),
            // We hold the block but not its ownership yet (e.g. a
            // replication fetch before merge). Defer to the owner.
            LocalRelease::OwnershipUnknown => None,
        })
    }

    /// Local release decision for the DEK stored under `cid`.
    ///
    /// Encryption blocks carry no identity (Go #4838): ownership is
    /// resolved through the block-CID -> DocID index, and the key is
    /// released if the actor may read ANY owning document (an encryption
    /// block can be co-owned by several documents). When ownership is not
    /// yet recorded — e.g. a block just fetched during replication, before
    /// its merge registers ownership — the node cannot authorize locally
    /// and defers to the owner via a remote fetch.
    async fn local_release_decision(
        &self,
        actor: Option<&Did>,
        cid: &EncryptionCid,
        delegated_actor: Option<(&Did, &str)>,
    ) -> Result<LocalRelease> {
        let doc_ids = self.doc_resolver.doc_ids_for_block(cid).await?;
        if doc_ids.is_empty() {
            return Ok(LocalRelease::OwnershipUnknown);
        }
        for doc_id in doc_ids {
            let scope = KeyScope::Document {
                doc_id,
                field: None,
            };
            if matches!(
                self.policy.check_release(actor, &scope).await?,
                PolicyDecision::Allow
            ) {
                return Ok(LocalRelease::Allow);
            }
            if let Some((delegate, collection_id)) = delegated_actor {
                if matches!(
                    self.policy
                        .check_delegated_release(delegate, &scope, collection_id)
                        .await?,
                    PolicyDecision::Allow
                ) {
                    return Ok(LocalRelease::Allow);
                }
            }
        }
        Ok(LocalRelease::Deny)
    }

    /// Serve-side release decision: the owner node has recorded ownership,
    /// so unknown ownership is treated as a denial (never leak a key we
    /// cannot attribute).
    async fn may_serve(
        &self,
        actor: Option<&Did>,
        cid: &EncryptionCid,
        delegated_actor: Option<(&Did, &str)>,
    ) -> Result<bool> {
        Ok(matches!(
            self.local_release_decision(actor, cid, delegated_actor)
                .await?,
            LocalRelease::Allow
        ))
    }

    fn remote_key(ctx: &RequestContext, cid: EncryptionCid) -> RemoteKey {
        RemoteKey {
            cid,
            capability: ctx.explicit_replay_capability().map(Arc::from),
        }
    }
}

/// Outcome of a local DEK release check.
enum LocalRelease {
    /// The actor may read an owning document; release the key.
    Allow,
    /// Ownership is known but the actor may read none of the owners.
    Deny,
    /// No owning document is recorded locally yet; defer to a remote fetch.
    OwnershipUnknown,
}

/// Why a peer request ended without a key, as the KMS reports it.
fn remote_error(error: inputs::Error<Error>) -> Error {
    match error {
        inputs::Error::Source(error) => error,
        other => Error::Remote(other.to_string()),
    }
}

/// Resolves which documents own a block, via the node's
/// block-CID -> DocID index (mirrors Go's `ResolveBlockDocIDs`).
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait BlockDocIDResolver: defra_core::thread_bounds::MaybeSendSync {
    async fn doc_ids_for_block(&self, cid: &EncryptionCid) -> Result<Vec<String>>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl KmsService for DefraKms {
    async fn get_keys(&self, ctx: &RequestContext, cids: &[EncryptionCid]) -> Result<KeyResults> {
        let answers = self.get_keys_each(ctx, cids).await?;
        let (results, tx) = KeyResults::new(cids.len().max(1));
        // The channel holds one item per CID, so no send waits.
        for (cid, answer) in cids.iter().zip(answers) {
            let _ = tx.send(answer.map(|key| (*cid, key))).await;
        }
        Ok(results)
    }

    async fn get_keys_each(
        &self,
        ctx: &RequestContext,
        cids: &[EncryptionCid],
    ) -> Result<Vec<Result<[u8; 32]>>> {
        // Local policy check uses the caller's actual identity (may be None
        // for anonymous callers). The node-identity fallback is only correct
        // on the WIRE path for gossip-triggered syncs (per Go PR #4778), not
        // on the local pass.
        let user_actor = ctx.user_identity();
        let mut answers: Vec<Option<Result<[u8; 32]>>> = Vec::with_capacity(cids.len());
        let mut remote = Vec::new();
        for (index, cid) in cids.iter().enumerate() {
            let answer = self.local_answer(user_actor, cid).await?;
            if answer.is_none() {
                remote.push((index, Self::remote_key(ctx, *cid)));
            }
            answers.push(answer);
        }

        if !remote.is_empty() {
            // No transports configured: report each key unavailable rather
            // than wait for a reply that can never come.
            let fetched: Vec<Result<[u8; 32]>> = if self.has_transports() {
                let keys = remote.iter().map(|(_, key)| key.clone()).collect();
                self.remote
                    .get_many(keys)
                    .await
                    .into_iter()
                    .map(|answer| answer.map(|key| **key).map_err(remote_error))
                    .collect()
            } else {
                remote.iter().map(|_| Err(Error::KeyUnavailable)).collect()
            };
            for ((index, _), answer) in remote.into_iter().zip(fetched) {
                answers[index] = Some(answer);
            }
        }
        Ok(answers
            .into_iter()
            .map(|answer| answer.unwrap_or(Err(Error::KeyUnavailable)))
            .collect())
    }

    async fn get_local_keys_each(
        &self,
        ctx: &RequestContext,
        cids: &[EncryptionCid],
    ) -> Result<Vec<Result<[u8; 32]>>> {
        let user_actor = ctx.user_identity();
        let mut answers = Vec::with_capacity(cids.len());
        for cid in cids {
            let answer = self.local_answer(user_actor, cid).await?;
            answers.push(answer.unwrap_or(Err(Error::KeyUnavailable)));
        }
        Ok(answers)
    }

    async fn prefetch_keys(&self, ctx: &RequestContext, cids: &[EncryptionCid]) {
        if !self.has_transports() {
            return;
        }
        let mut keys = Vec::with_capacity(cids.len());
        for cid in cids {
            match self.tier.get(cid).await {
                Ok(Some(_)) => {}
                Ok(None) => keys.push(Self::remote_key(ctx, *cid)),
                Err(error) => {
                    tracing::debug!(
                        cid = %cid,
                        %error,
                        "key store unreadable; asking peers for the key"
                    );
                    keys.push(Self::remote_key(ctx, *cid));
                }
            }
        }
        self.remote.prefetch(keys);
    }

    async fn generate_key(
        &self,
        _ctx: &RequestContext,
        scope: KeyScope,
    ) -> Result<(EncryptionCid, [u8; 32])> {
        let (cid, stored) = self.tier.store().generate(&scope).await?;
        let key = stored.key;
        self.tier.remember(cid, Arc::new(stored));
        Ok((cid, key))
    }

    async fn serve_request(
        &self,
        from: PeerIdentity,
        req: FetchEncryptionKeyRequest,
    ) -> Result<FetchEncryptionKeyReply> {
        let claimed_actor: Option<Did> = std::str::from_utf8(&req.identity)
            .ok()
            .and_then(|s| s.parse().ok());
        if claimed_actor.is_none() && !req.identity.is_empty() {
            tracing::warn!(
                identity_bytes_len = req.identity.len(),
                "KMS request identity field is non-empty but failed DID parse"
            );
        }
        if claimed_actor.as_ref() != from.authenticated_did.as_ref() {
            tracing::warn!(
                peer_id = %from.peer_id,
                "KMS request identity does not match authenticated peer identity"
            );
        }
        let actor = from.authenticated_did;
        let delegated_actor =
            from.explicit_replay_authorization
                .as_ref()
                .and_then(|authorization| {
                    authorization
                        .authorizer_did
                        .parse::<Did>()
                        .ok()
                        .map(|did| (did, authorization.collection_id.as_str()))
                });

        let mut out_links: Vec<Vec<u8>> = Vec::new();
        let mut out_blocks: Vec<Vec<u8>> = Vec::new();

        // ONE responder ephemeral per reply, shared across all served blocks
        // (matches Go). Its PUBLIC key is carried in the reply field below.
        let responder_eph = crypto::generate_x25519().map_err(|e| Error::Crypto(e.to_string()))?;
        let peer_id = self.local_peer_id();
        let aad = crate::ecies_envelope::make_associated_data(&req.ephemeral_public_key, &peer_id);

        for cid_bytes in req.links {
            let cid = match cid::Cid::try_from(cid_bytes.as_slice()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let Some(stored) = self.tier.get(&cid).await? else {
                continue;
            };
            if self
                .may_serve(
                    actor.as_ref(),
                    &cid,
                    delegated_actor
                        .as_ref()
                        .map(|(did, collection_id)| (did, *collection_id)),
                )
                .await?
            {
                tracing::debug!(cid = %cid, "KMS serve_request: DEK release GRANTED");
            } else {
                tracing::warn!(cid = %cid, "KMS serve_request: DEK release DENIED");
                continue;
            }
            let wrapped = crate::ecies_envelope::wrap_for_requester(
                &stored.block_bytes,
                &req.ephemeral_public_key,
                &responder_eph,
                &aad,
            )?;
            out_links.push(cid.to_bytes());
            out_blocks.push(wrapped);
        }

        Ok(FetchEncryptionKeyReply {
            links: out_links,
            blocks: out_blocks,
            ephemeral_public_key: x25519_dalek::PublicKey::from(&responder_eph)
                .as_bytes()
                .to_vec(),
        })
    }

    fn set_local_peer_id(&self, id: String) {
        self.local_peer_id.store(id);
    }
}

#[cfg(test)]
#[path = "defra_kms_tests.rs"]
mod tests;

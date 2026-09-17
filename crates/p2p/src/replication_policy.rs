//! App-facing replication policy: which blocks this node sends to which peer,
//! which peers it accepts pushes and sync requests from, which replicator
//! filters it evaluates, and which collection topics it joins.
//!
//! Every decision is node-local. It composes with the node's existing gates
//! (access mode, replicator registry, ACP serve gate, replicator filters) as an
//! AND: a policy can withhold what those gates allow, never allow what they
//! withhold. Withholding a block from a peer is always safe for replicated
//! state: the peer's merge path defers on the missing input.

use std::sync::Arc;

use async_trait::async_trait;
use cid::Cid;
use defra_core::thread_bounds::MaybeSendSync;
use identity::Did;

use crate::bitswap::LateBoundServeAcp;
use crate::replicator::ReplicationFilterMatcher;
use crate::transport::PeerId;

/// The remote peer a decision is about.
pub struct PolicyPeer<'a> {
    pub peer_id: &'a str,
    /// The DID the transport authenticated for the peer, when it has one.
    pub identity: Option<&'a Did>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundPath {
    /// A head this node announces to a replicator (live push, replay, retry).
    Push,
    /// A block a peer requested (CAR fetch, DocSync heads).
    Serve,
}

/// A block about to leave this node.
pub struct OutboundBlock<'a> {
    pub cid: &'a Cid,
    pub collection_id: &'a str,
    /// Documents the block belongs to; empty for a collection-level block.
    pub doc_ids: &'a [String],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundRequest {
    /// A PushLog request or gossip broadcast carrying a head.
    Push,
    /// A peer asking this node for heads (DocSync, BranchableSync).
    SyncRequest,
}

/// The app's replication policy. Every method defaults to today's behaviour.
///
/// A policy error withholds or refuses: serving is a confidentiality boundary
/// and fails closed.
///
/// # Scope
///
/// The policy gates the paths an iroh node replicates over: replicator pushes,
/// CAR and DocSync serving, PushLog and gossip ingress, and sync requests.
/// Bitswap serving (`crate::bitswap::filter`) is not gated, so a libp2p node
/// serving over Bitswap is outside the policy's reach.
///
/// # What withholding does not hide
///
/// Gossip broadcasts are per topic, not per peer. Every subscriber of a
/// collection or document topic, including a peer this policy withholds from,
/// sees the head CIDs announced there. It cannot fetch the blocks, but it
/// learns that the document exists, that it changed, and its head CID. To keep
/// a peer from learning that, keep it off the topic.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ReplicationPolicy: MaybeSendSync {
    /// Whether `block` may go to `peer`.
    ///
    /// Withholding a push does not drop it: the durable retry marker for the
    /// peer and document stays, and the retry clock asks again with backoff,
    /// so a later, more permissive answer delivers the head. Withholding a
    /// serve drops only that response; the peer may request again.
    async fn may_send(
        &self,
        _peer: &PolicyPeer<'_>,
        _path: OutboundPath,
        _block: &OutboundBlock<'_>,
    ) -> Result<bool, String> {
        Ok(true)
    }

    async fn may_accept(
        &self,
        _peer: &PolicyPeer<'_>,
        _request: InboundRequest,
        _collection_id: &str,
    ) -> Result<bool, String> {
        Ok(true)
    }

    /// The matcher for replicator filters, replacing the query-filter matcher.
    fn filter_matcher(&self) -> Option<Arc<dyn ReplicationFilterMatcher>> {
        None
    }

    /// Collection names this node joins the topics of at startup.
    fn collections(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Today's behaviour: allow everything the existing gates allow.
pub struct DefaultReplicationPolicy;

impl ReplicationPolicy for DefaultReplicationPolicy {}

/// The installed policy, bound to the transport's peer identity resolver.
/// With no policy installed every check passes without resolving anything.
#[derive(Default)]
pub struct ReplicationPolicyGate {
    policy: std::sync::OnceLock<Arc<dyn ReplicationPolicy>>,
    serve_acp: Arc<LateBoundServeAcp>,
}

impl ReplicationPolicyGate {
    pub fn new(serve_acp: Arc<LateBoundServeAcp>) -> Self {
        Self {
            policy: std::sync::OnceLock::new(),
            serve_acp,
        }
    }

    /// First call wins. Install before the transport handles traffic.
    pub fn set(&self, policy: Arc<dyn ReplicationPolicy>) {
        let _ = self.policy.set(policy);
    }

    pub fn is_installed(&self) -> bool {
        self.policy.get().is_some()
    }

    async fn identity(&self, peer_id: &str) -> Option<Did> {
        let serve = self.serve_acp.get()?;
        serve
            .resolver
            .resolve(&PeerId::new(peer_id.to_string()))
            .await
    }

    pub async fn may_send(
        &self,
        peer_id: &str,
        path: OutboundPath,
        block: &OutboundBlock<'_>,
    ) -> bool {
        let Some(policy) = self.policy.get() else {
            return true;
        };
        let identity = self.identity(peer_id).await;
        let peer = PolicyPeer {
            peer_id,
            identity: identity.as_ref(),
        };
        policy
            .may_send(&peer, path, block)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%peer_id, cid = %block.cid, %error, "Replication policy failed; withholding block");
                false
            })
    }

    pub async fn may_accept(
        &self,
        peer_id: &str,
        request: InboundRequest,
        collection_id: &str,
    ) -> bool {
        let Some(policy) = self.policy.get() else {
            return true;
        };
        let identity = self.identity(peer_id).await;
        let peer = PolicyPeer {
            peer_id,
            identity: identity.as_ref(),
        };
        policy
            .may_accept(&peer, request, collection_id)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%peer_id, %collection_id, %error, "Replication policy failed; refusing peer");
                false
            })
    }
}

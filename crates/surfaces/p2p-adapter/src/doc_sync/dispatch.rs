use crate::P2PResult;
use async_trait::async_trait;
use p2p::message::DocSyncRequest;

use crate::{P2PError, P2PErrorExt as _};

/// What `sync_documents` needs from a transport in order to reach peers.
///
/// Narrow on purpose: the transport's peer-id type and how a request is signed
/// do not belong in the sync logic, and a test drives it with a double.
///
/// pinc's `send_doc_sync_request` is a request-response with a 30s timeout,
/// so its `Ok` means the peer replied.
#[async_trait]
pub(crate) trait DocSyncDispatch: Send + Sync {
    /// Peer identifier.
    type Peer: Send + Sync;

    /// Whether `Ok` from `send_doc_sync_request` proves the peer answered.
    ///
    /// True on pinc, whose send is a request-response: `Err` there means the
    /// peer stayed silent, which is Go's `pendingPeers` condition. False on a
    /// fire-and-forget send, where a failure means the bytes did not leave,
    /// not that the peer would have declined to reply.
    const SEND_CONFIRMS_REPLY: bool;

    async fn connected_peers(&self) -> P2PResult<Vec<Self::Peer>>;

    /// Signs in place using whatever identity the transport already uses.
    fn sign_request(&self, request: &mut DocSyncRequest) -> P2PResult<()>;

    async fn send_doc_sync_request(
        &self,
        peer: &Self::Peer,
        request: DocSyncRequest,
    ) -> P2PResult<()>;
}

#[cfg(feature = "pinc")]
#[async_trait]
impl DocSyncDispatch for defra_pinc::PincTransport {
    type Peer = p2p::transport::PeerId;

    const SEND_CONFIRMS_REPLY: bool = true;

    async fn connected_peers(&self) -> P2PResult<Vec<Self::Peer>> {
        p2p::P2PTransport::connected_peers(self)
            .await
            .map_err(|error| P2PError::transport(format!("failed to get connected peers: {error}")))
    }

    fn sign_request(&self, request: &mut DocSyncRequest) -> P2PResult<()> {
        p2p::signing::sign_with_transport(self, request)
            .map_err(|error| P2PError::internal(format!("failed to sign DocSync request: {error}")))
    }

    async fn send_doc_sync_request(
        &self,
        peer: &Self::Peer,
        request: DocSyncRequest,
    ) -> P2PResult<()> {
        p2p::P2PTransport::send_doc_sync_request(self, peer, request)
            .await
            .map_err(|error| P2PError::transport(error.to_string()))
    }
}

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::P2PResult;
use async_trait::async_trait;
use p2p::message::DocSyncRequest;

use super::dispatch::DocSyncDispatch;
use crate::{P2PError, P2PErrorExt as _};

/// Reports `peer_count` connected peers and fails every send, so
/// `sync_documents`'s `!any_sent` branch is reachable without a live
/// transport or a real timeout.
pub(crate) struct FailingDispatch {
    peer_count: usize,
    pub(crate) send_attempts: AtomicUsize,
}

impl FailingDispatch {
    pub(crate) fn with_peers(peer_count: usize) -> Self {
        Self {
            peer_count,
            send_attempts: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl DocSyncDispatch for FailingDispatch {
    type Peer = String;

    const SEND_CONFIRMS_REPLY: bool = false;

    async fn connected_peers(&self) -> P2PResult<Vec<Self::Peer>> {
        Ok((0..self.peer_count).map(|i| format!("peer-{i}")).collect())
    }

    fn sign_request(&self, _request: &mut DocSyncRequest) -> P2PResult<()> {
        Ok(())
    }

    async fn send_doc_sync_request(
        &self,
        _peer: &Self::Peer,
        _request: DocSyncRequest,
    ) -> P2PResult<()> {
        self.send_attempts.fetch_add(1, Ordering::SeqCst);
        Err(P2PError::transport("send failed"))
    }
}

/// Reports `peer_count` connected peers and sends successfully to all but the
/// last `failing_peers` of them, so "one peer answered, one stayed silent" is
/// reachable without a live transport. `CONFIRMS` selects the transport's
/// reply semantics.
pub(crate) struct MixedDispatch<const CONFIRMS: bool> {
    peer_count: usize,
    failing_peers: usize,
}

impl<const CONFIRMS: bool> MixedDispatch<CONFIRMS> {
    pub(crate) fn new(peer_count: usize, failing_peers: usize) -> Self {
        Self {
            peer_count,
            failing_peers,
        }
    }
}

#[async_trait]
impl<const CONFIRMS: bool> DocSyncDispatch for MixedDispatch<CONFIRMS> {
    type Peer = String;

    const SEND_CONFIRMS_REPLY: bool = CONFIRMS;

    async fn connected_peers(&self) -> P2PResult<Vec<Self::Peer>> {
        Ok((0..self.peer_count).map(|i| format!("peer-{i}")).collect())
    }

    fn sign_request(&self, _request: &mut DocSyncRequest) -> P2PResult<()> {
        Ok(())
    }

    async fn send_doc_sync_request(
        &self,
        peer: &Self::Peer,
        _request: DocSyncRequest,
    ) -> P2PResult<()> {
        let index: usize = peer
            .trim_start_matches("peer-")
            .parse()
            .expect("test peers are named peer-N");
        if index >= self.peer_count - self.failing_peers {
            return Err(P2PError::transport("peer stayed silent"));
        }
        Ok(())
    }
}

/// Reports `peer_count` connected peers and takes `delay` to send to each, so
/// a dispatch that ignores the caller's remaining budget shows up as wall
/// clock rather than as a wrong return value.
pub(crate) struct SlowDispatch {
    peer_count: usize,
    delay: Duration,
}

impl SlowDispatch {
    pub(crate) fn new(peer_count: usize, delay: Duration) -> Self {
        Self { peer_count, delay }
    }
}

#[async_trait]
impl DocSyncDispatch for SlowDispatch {
    type Peer = String;

    const SEND_CONFIRMS_REPLY: bool = true;

    async fn connected_peers(&self) -> P2PResult<Vec<Self::Peer>> {
        Ok((0..self.peer_count).map(|i| format!("peer-{i}")).collect())
    }

    fn sign_request(&self, _request: &mut DocSyncRequest) -> P2PResult<()> {
        Ok(())
    }

    async fn send_doc_sync_request(
        &self,
        _peer: &Self::Peer,
        _request: DocSyncRequest,
    ) -> P2PResult<()> {
        p2p::rt::time::sleep(self.delay).await;
        Ok(())
    }
}

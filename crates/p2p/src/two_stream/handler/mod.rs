//! Two-stream protocol handler.
//!
//! Go's DefraDB uses a two-stream pattern for request-response:
//! 1. Sender opens stream on `/defradb/rep_req/0.0.1`, sends request, closes stream
//! 2. Receiver processes request, opens NEW stream on `/defradb/rep_resp/0.0.1` to send response
//!
//! This is different from libp2p-rust's request-response which uses bidirectional streams.
//! This module implements Go's pattern for interoperability using libp2p-stream.

mod branchable_se;
mod car;
mod doc_sync;
mod identity;
mod inbound;
mod manage;
mod pushlog;
mod se_query;

use rapidhash::fast::RandomState;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::AsyncReadExt;
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use libp2p::Stream;

use libp2p::StreamProtocol;
use libp2p_stream as stream;
use tokio::sync::oneshot;

use libp2p::PeerId;

use crate::message::{DocSyncReply, IdentityResponse, PushLogReply, PushLogRequest};
use crate::protocol::{
    CAR_REQUEST_PROTOCOL, CAR_RESPONSE_PROTOCOL, IDENTITY_REQUEST_PROTOCOL,
    IDENTITY_RESPONSE_PROTOCOL, MANAGE_QUERY_REQUEST_PROTOCOL, MANAGE_QUERY_RESPONSE_PROTOCOL,
    MANAGE_REQUEST_PROTOCOL, MANAGE_RESPONSE_PROTOCOL, REP_REQUEST_PROTOCOL, REP_RESPONSE_PROTOCOL,
    SE_QUERY_REQUEST_PROTOCOL, SE_QUERY_RESPONSE_PROTOCOL, SE_REQUEST_PROTOCOL,
    SE_RESPONSE_PROTOCOL,
};
use crate::{error::Error, message::Message, Result};

/// Timeout for waiting for a response.
pub(super) const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Pending response key bound to the expected transport peer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PendingResponseKey {
    pub(crate) peer_id: PeerId,
    pub(crate) message_id: String,
}

impl PendingResponseKey {
    pub(crate) fn new(peer_id: PeerId, message_id: impl Into<String>) -> Self {
        Self {
            peer_id,
            message_id: message_id.into(),
        }
    }
}

pub(super) fn ensure_transport_sender<M: Message>(peer_id: &PeerId, msg: &M) -> Result<()> {
    if msg.sender_id() == peer_id.to_string() {
        Ok(())
    } else {
        Err(Error::Transport(format!(
            "transport peer {} did not match signed sender {}",
            peer_id,
            msg.sender_id()
        )))
    }
}

/// A reply sender parked in a queue so the responder can take ownership of it.
type ReplySlot<T> = Arc<SegQueue<oneshot::Sender<T>>>;
type ReplyTable<T> = HopscotchMap<PendingResponseKey, ReplySlot<T>, RandomState>;
type RequestTable = HopscotchMap<PendingResponseKey, Instant, RandomState>;

fn reply_slot<T: 'static>(sender: oneshot::Sender<T>) -> ReplySlot<T> {
    let slot = SegQueue::new();
    slot.push(sender);
    Arc::new(slot)
}

fn register<T: 'static>(
    table: &ReplyTable<T>,
    key: PendingResponseKey,
    sender: oneshot::Sender<T>,
) {
    table.insert(key, reply_slot(sender));
}

fn take<T: 'static>(table: &ReplyTable<T>, key: &PendingResponseKey) -> Option<oneshot::Sender<T>> {
    table.remove(key).and_then(|slot| slot.pop())
}

fn prune_expired(table: &RequestTable) {
    let now = Instant::now();
    let expired: Vec<_> = table
        .iter()
        .filter(|(_, inserted_at)| now.duration_since(*inserted_at) > RESPONSE_TIMEOUT)
        .map(|(key, _)| key)
        .collect();
    for key in expired {
        table.remove(&key);
    }
}

/// State for tracking pending responses. Each table stands alone: no reply
/// is routed through more than one of them.
pub(crate) struct PendingResponses {
    /// Expected peer + MessageID to PushLog response channel.
    channels: ReplyTable<PushLogReply>,
    /// Expected peer + MessageID to identity response channel.
    identity_channels: ReplyTable<IdentityResponse>,
    /// Expected peer + MessageID to DocSync response channel.
    doc_sync_channels: ReplyTable<DocSyncReply>,
    /// Fire-and-forget DocSync requests awaiting an async response event.
    doc_sync_requests: RequestTable,
    /// Fire-and-forget BranchableSync requests awaiting an async response event.
    branchable_sync_requests: RequestTable,
}

impl Default for PendingResponses {
    fn default() -> Self {
        Self {
            channels: HopscotchMap::with_hasher(RandomState::default()),
            identity_channels: HopscotchMap::with_hasher(RandomState::default()),
            doc_sync_channels: HopscotchMap::with_hasher(RandomState::default()),
            doc_sync_requests: HopscotchMap::with_hasher(RandomState::default()),
            branchable_sync_requests: HopscotchMap::with_hasher(RandomState::default()),
        }
    }
}

impl PendingResponses {
    pub(crate) fn register_pushlog(
        &self,
        key: PendingResponseKey,
        sender: oneshot::Sender<PushLogReply>,
    ) {
        register(&self.channels, key, sender);
    }

    pub(crate) fn has_pushlog(&self, key: &PendingResponseKey) -> bool {
        self.channels.contains_key(key)
    }

    pub(crate) fn take_pushlog(
        &self,
        key: &PendingResponseKey,
    ) -> Option<oneshot::Sender<PushLogReply>> {
        take(&self.channels, key)
    }

    pub(crate) fn register_identity(
        &self,
        key: PendingResponseKey,
        sender: oneshot::Sender<IdentityResponse>,
    ) {
        register(&self.identity_channels, key, sender);
    }

    pub(crate) fn take_identity(
        &self,
        key: &PendingResponseKey,
    ) -> Option<oneshot::Sender<IdentityResponse>> {
        take(&self.identity_channels, key)
    }

    pub(crate) fn register_doc_sync(
        &self,
        key: PendingResponseKey,
        sender: oneshot::Sender<DocSyncReply>,
    ) {
        register(&self.doc_sync_channels, key, sender);
    }

    pub(crate) fn take_doc_sync(
        &self,
        key: &PendingResponseKey,
    ) -> Option<oneshot::Sender<DocSyncReply>> {
        take(&self.doc_sync_channels, key)
    }

    pub(crate) fn register_doc_sync_request(&self, key: PendingResponseKey) {
        prune_expired(&self.doc_sync_requests);
        self.doc_sync_requests.insert(key, Instant::now());
    }

    pub(crate) fn consume_doc_sync_request(&self, key: &PendingResponseKey) -> bool {
        prune_expired(&self.doc_sync_requests);
        self.doc_sync_requests.remove(key).is_some()
    }

    pub(crate) fn register_branchable_sync_request(&self, key: PendingResponseKey) {
        prune_expired(&self.branchable_sync_requests);
        self.branchable_sync_requests.insert(key, Instant::now());
    }

    pub(crate) fn consume_branchable_sync_request(&self, key: &PendingResponseKey) -> bool {
        prune_expired(&self.branchable_sync_requests);
        self.branchable_sync_requests.remove(key).is_some()
    }
}

/// Two-stream protocol handler.
///
/// Handles Go's two-stream request-response pattern where requests and responses
/// flow on separate streams identified by different protocol IDs.
///
/// Uses `libp2p-stream` for stream management.
#[derive(Clone)]
pub struct TwoStreamHandler {
    /// Control for the stream behaviour (for opening streams).
    pub(super) control: stream::Control,
    /// Pending response channels keyed by expected peer and MessageID.
    pub(super) pending: Arc<PendingResponses>,
}

impl TwoStreamHandler {
    /// Create a new two-stream handler from a stream::Control.
    pub fn new(control: stream::Control) -> Self {
        Self {
            control,
            pending: Arc::new(PendingResponses::default()),
        }
    }

    /// Get a clone of the pending responses Arc for lock-free response processing.
    pub(crate) fn pending_responses(&self) -> Arc<PendingResponses> {
        self.pending.clone()
    }

    /// Get the request protocol.
    pub fn request_protocol() -> StreamProtocol {
        StreamProtocol::new(REP_REQUEST_PROTOCOL)
    }

    pub fn retry_request_protocol() -> StreamProtocol {
        StreamProtocol::new(crate::protocol::REP_RETRY_REQUEST_PROTOCOL)
    }

    /// Get the response protocol.
    pub fn response_protocol() -> StreamProtocol {
        StreamProtocol::new(REP_RESPONSE_PROTOCOL)
    }

    /// Get the SE request protocol.
    pub fn se_request_protocol() -> StreamProtocol {
        StreamProtocol::new(SE_REQUEST_PROTOCOL)
    }

    /// Get the SE response protocol.
    pub fn se_response_protocol() -> StreamProtocol {
        StreamProtocol::new(SE_RESPONSE_PROTOCOL)
    }

    /// Get the SE query request protocol.
    pub fn se_query_request_protocol() -> StreamProtocol {
        StreamProtocol::new(SE_QUERY_REQUEST_PROTOCOL)
    }

    /// Get the SE query response protocol.
    pub fn se_query_response_protocol() -> StreamProtocol {
        StreamProtocol::new(SE_QUERY_RESPONSE_PROTOCOL)
    }

    /// Get the management mutate request protocol.
    pub fn manage_request_protocol() -> StreamProtocol {
        StreamProtocol::new(MANAGE_REQUEST_PROTOCOL)
    }

    /// Get the management mutate response protocol.
    pub fn manage_response_protocol() -> StreamProtocol {
        StreamProtocol::new(MANAGE_RESPONSE_PROTOCOL)
    }

    /// Get the management query request protocol.
    pub fn manage_query_request_protocol() -> StreamProtocol {
        StreamProtocol::new(MANAGE_QUERY_REQUEST_PROTOCOL)
    }

    /// Get the management query response protocol.
    pub fn manage_query_response_protocol() -> StreamProtocol {
        StreamProtocol::new(MANAGE_QUERY_RESPONSE_PROTOCOL)
    }

    /// Get the CAR request protocol.
    pub fn car_request_protocol() -> StreamProtocol {
        StreamProtocol::new(CAR_REQUEST_PROTOCOL)
    }

    /// Get the CAR response protocol.
    pub fn car_response_protocol() -> StreamProtocol {
        StreamProtocol::new(CAR_RESPONSE_PROTOCOL)
    }

    /// Get the identity request protocol.
    pub fn identity_request_protocol() -> StreamProtocol {
        StreamProtocol::new(IDENTITY_REQUEST_PROTOCOL)
    }

    /// Get the identity response protocol.
    pub fn identity_response_protocol() -> StreamProtocol {
        StreamProtocol::new(IDENTITY_RESPONSE_PROTOCOL)
    }

    /// Clean up a pending response channel (used on timeout or cancellation).
    pub fn cleanup_pending(&self, peer_id: PeerId, message_id: &str) {
        drop(
            self.pending
                .take_pushlog(&PendingResponseKey::new(peer_id, message_id)),
        );
    }

    /// Clean up a pending identity response channel (used on timeout or cancellation).
    pub fn cleanup_pending_identity(&self, peer_id: PeerId, message_id: &str) {
        drop(
            self.pending
                .take_identity(&PendingResponseKey::new(peer_id, message_id)),
        );
    }

    /// Clean up a pending DocSync request.
    pub fn cleanup_pending_doc_sync(&self, peer_id: PeerId, message_id: &str) {
        let key = PendingResponseKey::new(peer_id, message_id);
        drop(self.pending.take_doc_sync(&key));
        self.pending.doc_sync_requests.remove(&key);
    }

    /// Clean up a pending BranchableSync request.
    pub fn cleanup_pending_branchable_sync(&self, peer_id: PeerId, message_id: &str) {
        self.pending
            .branchable_sync_requests
            .remove(&PendingResponseKey::new(peer_id, message_id));
    }

    /// Create a success reply for a request.
    pub fn success_reply(request: &PushLogRequest) -> PushLogReply {
        PushLogReply::success(&request.message_id)
    }

    /// Create an error reply for a request.
    pub fn error_reply(request: &PushLogRequest, error: &str) -> PushLogReply {
        PushLogReply::error(&request.message_id, error)
    }
}

/// Read a bounded, timed CBOR message from a stream.
///
/// Reads at most `max_msg_size` bytes within `stream_read_timeout`, then
/// deserializes the buffer as CBOR into `T`.
pub(super) async fn read_cbor_message<T>(
    peer_id: PeerId,
    mut stream: Stream,
    max_msg_size: u64,
    stream_read_timeout: std::time::Duration,
    label: &'static str,
) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let mut buf = Vec::new();
    tokio::time::timeout(
        stream_read_timeout,
        (&mut stream).take(max_msg_size).read_to_end(&mut buf),
    )
    .await
    .map_err(|_| {
        tracing::warn!(peer_id = %peer_id, "{label} stream read timed out");
        Error::CborDeserialization(format!("{label} stream read timed out"))
    })?
    .map_err(|e| Error::CborDeserialization(format!("failed to read {label}: {e}")))?;

    defra_core::cbor::from_slice(&buf)
        .map_err(|e| Error::CborDeserialization(format!("failed to decode {label}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_sync_pending_requests_are_peer_bound_and_single_use() {
        let peer = PeerId::random();
        let other_peer = PeerId::random();
        let key = PendingResponseKey::new(peer, "doc-sync-1");
        let wrong_peer_key = PendingResponseKey::new(other_peer, "doc-sync-1");
        let pending = PendingResponses::default();

        pending.register_doc_sync_request(key.clone());

        assert!(!pending.consume_doc_sync_request(&wrong_peer_key));
        assert!(pending.consume_doc_sync_request(&key));
        assert!(!pending.consume_doc_sync_request(&key));
    }

    #[test]
    fn branchable_sync_pending_requests_are_peer_bound_and_single_use() {
        let peer = PeerId::random();
        let other_peer = PeerId::random();
        let key = PendingResponseKey::new(peer, "branchable-sync-1");
        let wrong_peer_key = PendingResponseKey::new(other_peer, "branchable-sync-1");
        let pending = PendingResponses::default();

        pending.register_branchable_sync_request(key.clone());

        assert!(!pending.consume_branchable_sync_request(&wrong_peer_key));
        assert!(pending.consume_branchable_sync_request(&key));
        assert!(!pending.consume_branchable_sync_request(&key));
    }
}

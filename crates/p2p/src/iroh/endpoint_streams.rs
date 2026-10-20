//! Incoming connection and stream processing for the iroh endpoint.

use std::sync::Arc;

use iroh::endpoint::Connection;
use iroh::EndpointId;
use tracing::{debug, warn};

use crate::error::Error;
use crate::message::{Message, PushLogReply};
use crate::sync::DispatchSender;
use crate::transport::{PeerId, TransportEvent};

use super::endpoint::{spawn_task, EndpointResources, PendingPushLogReplies, SpawnedTasks};
use super::inbound_budget::{
    body_deadline, ConnectionShare, InboundBudget, InboundLease, ADMIT_DEADLINE, BODY_IDLE_DEADLINE,
};
use super::peer_map::{endpoint_id_to_peer_id, SharedPeerMap};
use super::protocols;
use super::records::{Input as RecordsInput, RecordsHandle, RecordsMessage};
use super::services::{self, Service};

/// QUIC application error code closing a connection refused by the inbound
/// allowlist. Distinct from the disconnect code (0) used elsewhere.
const REFUSED_CONNECTION_CODE: u32 = 1;

/// QUIC stream error codes sent when a stream is refused before its body is
/// read, or reset because the body stopped arriving.
const RESET_UNKNOWN_SERVICE: u32 = 2;
const RESET_MESSAGE_TOO_LARGE: u32 = 3;
const RESET_BUSY: u32 = 4;
const RESET_STALLED: u32 = 5;

/// Handle an incoming QUIC connection.
pub(super) async fn handle_incoming(
    incoming: iroh::endpoint::Incoming,
    resources: &EndpointResources,
    pending_pushlog_replies: &PendingPushLogReplies,
    event_tx: &DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
) {
    let remote_addr = match incoming.remote_addr() {
        iroh::endpoint::IncomingAddr::Ip(addr) => Some(addr),
        _ => None,
    };

    // Accept the connection
    let connection = match incoming.accept() {
        Ok(accepting) => match accepting.await {
            Ok(conn) => conn,
            Err(e) => {
                let error_msg = e.to_string();
                if crate::error::Error::is_connection_loss_reason(&error_msg) {
                    debug!(
                        remote_addr = ?remote_addr,
                        error = %error_msg,
                        "Incoming connection handshake ended before completion"
                    );
                } else {
                    warn!(
                        remote_addr = ?remote_addr,
                        error = %error_msg,
                        "Failed to complete connection handshake"
                    );
                }
                return;
            }
        },
        Err(e) => {
            warn!("Failed to accept connection: {}", e);
            return;
        }
    };

    // Establish identity from the accepted connection itself (the iroh
    // endpoint id authenticated by the QUIC/TLS handshake), never from
    // anything the caller sends in a payload. A refused peer is closed here,
    // before it reaches the mux layer below.
    let remote_id = connection.remote_id();
    if !resources.admission.admits_inbound(&remote_id) {
        warn!(
            peer_id = %endpoint_id_to_peer_id(&remote_id),
            "Refused inbound Iroh connection: peer is not admitted"
        );
        connection.close(REFUSED_CONNECTION_CODE.into(), b"peer not authorized");
        return;
    }

    let is_new =
        resources
            .peer_map
            .increment_connections(remote_id, remote_addr, connection.clone());

    // Re-check AFTER registering, not only before. A revoke that ran between
    // the check above and this registration scanned `peer_map` while this
    // connection was still absent from it, so it closed nothing. Publishing
    // the handle first and re-checking second means one of the two always
    // sees the other. The stream task is still spawned below on the refused
    // path, so its cleanup decrements the count this registration added.
    let admitted = resources.admission.admits_inbound(&remote_id);
    if !admitted {
        warn!(
            peer_id = %endpoint_id_to_peer_id(&remote_id),
            "Closing inbound Iroh connection: peer was revoked while it was being accepted"
        );
        connection.close(REFUSED_CONNECTION_CODE.into(), b"peer not authorized");
    }

    if admitted
        && is_new
        && event_tx
            .send(TransportEvent::PeerConnected(endpoint_id_to_peer_id(
                &remote_id,
            )))
            .await
            .is_err()
    {
        warn!("Event channel closed, cannot emit PeerConnected");
    }

    if admitted {
        resources.gossip.connected(remote_id, &connection).await;
    }

    // Spawn handler for this connection's streams
    let context = ConnectionStreamContext::new(
        resources,
        Arc::clone(pending_pushlog_replies),
        event_tx.clone(),
    );
    let _ = spawn_task(&resources.spawned_tasks, async move {
        handle_connection_streams(connection, remote_id, context).await;
    });
}

/// Shared state for every stream accepted on one authenticated connection.
#[derive(Clone)]
pub(super) struct ConnectionStreamContext {
    event_tx: DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
    peer_map: Arc<SharedPeerMap>,
    pending_pushlog_replies: PendingPushLogReplies,
    node_identity: Option<Arc<identity::RawIdentity>>,
    spawned_tasks: SpawnedTasks,
    inbound_budget: Arc<InboundBudget>,
    share: Arc<ConnectionShare>,
    records: RecordsHandle,
}

impl ConnectionStreamContext {
    pub(super) fn new(
        resources: &EndpointResources,
        pending_pushlog_replies: PendingPushLogReplies,
        event_tx: DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
    ) -> Self {
        Self {
            event_tx,
            peer_map: Arc::clone(&resources.peer_map),
            pending_pushlog_replies,
            node_identity: resources.node_identity.clone(),
            spawned_tasks: Arc::clone(&resources.spawned_tasks),
            inbound_budget: Arc::clone(&resources.inbound_budget),
            share: Arc::new(resources.inbound_budget.connection_share()),
            records: resources.records.clone(),
        }
    }
}

/// Process streams on an accepted connection, dispatching by stream tag.
///
/// The tag is read inside the spawned task, not the accept loop, so a peer that
/// stalls before writing one cannot hold up the other protocols.
///
/// Emits `PeerDisconnected` only when the last connection for this peer closes.
pub(super) async fn handle_connection_streams(
    connection: Connection,
    remote_id: EndpointId,
    context: ConnectionStreamContext,
) {
    let peer_id = endpoint_id_to_peer_id(&remote_id);

    while let Ok((send, mut recv)) = connection.accept_bi().await {
        let peer_id = peer_id.clone();
        let event_tx = context.event_tx.clone();
        let pending_pushlog_replies = context.pending_pushlog_replies.clone();
        let node_identity = context.node_identity.clone();
        let inbound_budget = Arc::clone(&context.inbound_budget);
        let share = Arc::clone(&context.share);
        let records = context.records.clone();
        let _ = spawn_task(&context.spawned_tasks, async move {
            let Some((service, payload, lease)) =
                admit_stream(&mut recv, &inbound_budget, &share, &peer_id).await
            else {
                return;
            };
            // Replies on this stream go out at the service's priority.
            let _ = send.set_priority(service.priority.send_priority());
            if service.tag == protocols::STREAM_SERVICE_RECORDS {
                match protocols::decode::<RecordsMessage, _>(payload) {
                    Ok(message) => records.push(RecordsInput::Message {
                        from: remote_id,
                        message,
                    }),
                    Err(e) => debug!("Undecodable records message from {}: {}", peer_id, e),
                }
                drop(lease);
                return;
            }
            if let Err(e) = dispatch_stream(
                service.tag,
                &peer_id,
                send,
                payload,
                lease,
                &event_tx,
                &pending_pushlog_replies,
                node_identity.as_deref(),
            )
            .await
            {
                debug!("Stream error from {}: {}", peer_id, e);
            }
        });
    }

    let fully_disconnected = context.peer_map.decrement_connections(&remote_id);
    debug!(peer_id = %peer_id, fully_disconnected, "Connection closed");

    if fully_disconnected
        && context
            .event_tx
            .send(TransportEvent::PeerDisconnected(peer_id))
            .await
            .is_err()
    {
        debug!("Event channel closed, cannot emit PeerDisconnected");
    }
}

/// Read a stream's tag and length, then wait for room for its body. Resets
/// the stream instead when the tag is unknown, the message is too large for
/// its tag, or no room frees up before the deadline.
async fn admit_stream(
    recv: &mut iroh::endpoint::RecvStream,
    budget: &InboundBudget,
    share: &ConnectionShare,
    peer_id: &PeerId,
) -> Option<(&'static Service, Vec<u8>, InboundLease)> {
    let tag = protocols::read_stream_tag(recv)
        .await
        .inspect_err(|e| debug!(peer_id = %peer_id, error = %e, "Unreadable stream tag"))
        .ok()?;
    let Some(service) = services::lookup(&tag) else {
        debug!(peer_id = %peer_id, tag = %String::from_utf8_lossy(&tag), "Unknown stream tag");
        let _ = recv.stop(RESET_UNKNOWN_SERVICE.into());
        return None;
    };
    let len = protocols::read_length(recv)
        .await
        .inspect_err(|e| debug!(peer_id = %peer_id, error = %e, "Unreadable message length"))
        .ok()?;
    if len > service.max_message {
        budget.record_oversized(service.priority);
        debug!(peer_id = %peer_id, len, limit = service.max_message, "Message too large for its stream tag");
        let _ = recv.stop(RESET_MESSAGE_TOO_LARGE.into());
        return None;
    }
    let Ok(lease) = budget
        .admit(share, service.priority, len, ADMIT_DEADLINE)
        .await
    else {
        debug!(peer_id = %peer_id, len, class = service.priority.name(), "No inbound room before the deadline");
        let _ = recv.stop(RESET_BUSY.into());
        return None;
    };
    // The idle deadline catches a sender that stops; the total deadline catches
    // one that trickles bytes to keep its lease.
    let payload = n0_future::time::timeout(
        body_deadline(len),
        protocols::read_body_until_idle(recv, len, BODY_IDLE_DEADLINE),
    )
    .await
    .unwrap_or(Ok(None))
    .inspect_err(|e| debug!(peer_id = %peer_id, error = %e, "Unreadable message body"))
    .ok()?;
    let Some(payload) = payload else {
        debug!(peer_id = %peer_id, len, "Message body stopped arriving");
        let _ = recv.stop(RESET_STALLED.into());
        return None;
    };
    Some((service, payload, lease))
}

/// Dispatch a stream based on the tag its opener wrote. Every event it emits
/// carries the body's lease, so the room is held until the event is handled.
#[allow(clippy::too_many_arguments)]
async fn dispatch_stream(
    tag: &[u8],
    peer_id: &PeerId,
    send: iroh::endpoint::SendStream,
    payload: Vec<u8>,
    lease: InboundLease,
    event_tx: &DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
    pending_pushlog_replies: &PendingPushLogReplies,
    node_identity: Option<&identity::RawIdentity>,
) -> crate::error::Result<()> {
    match tag {
        x if x == protocols::STREAM_IDENTITY => {
            let request: crate::message::IdentityRequest = protocols::decode(payload)?;
            let mut send = send;
            let response = if request.peer_id != peer_id.as_str() {
                crate::message::IdentityResponse::error(
                    &request.message_id,
                    "identity request peer did not match authenticated endpoint",
                )
            } else if let Some(node_identity) = node_identity {
                match identity::new_token(
                    node_identity,
                    std::time::Duration::from_secs(60 * 60 * 24),
                    Some(peer_id.to_string()),
                    None,
                ) {
                    Ok(token) => {
                        crate::message::IdentityResponse::success(&request.message_id, token)
                    }
                    Err(error) => crate::message::IdentityResponse::error(
                        &request.message_id,
                        &format!("failed to generate identity token: {error}"),
                    ),
                }
            } else {
                crate::message::IdentityResponse::identity_unconfigured(&request.message_id)
            };
            protocols::write_message(&mut send, &response).await?;
            send.finish()
                .map_err(|error| Error::Transport(error.to_string()))?;
        }
        x if x == protocols::STREAM_PUSHLOG => {
            let request: crate::message::PushLogRequest = protocols::decode(payload)?;
            if event_tx
                .send_leased(
                    TransportEvent::PushLogRequest {
                        peer_id: peer_id.clone(),
                        request,
                        token: send,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit PushLogRequest");
            }
        }
        x if x == protocols::STREAM_TWOSTREAM => {
            let request: crate::message::PushLogRequest = protocols::decode(payload)?;
            if event_tx
                .send_leased(
                    TransportEvent::TwoStreamRequest {
                        peer_id: peer_id.clone(),
                        request,
                        token: Some(send),
                        is_explicit_replicator: false,
                        explicit_replay_authorization: None,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit TwoStreamRequest");
            }
        }
        x if x == protocols::STREAM_TWOSTREAM_RESP => {
            // The reverse-stream ACK, for a request that did not advertise
            // same-stream reply support.
            let reply: PushLogReply = protocols::decode(payload)?;
            let sender = pending_pushlog_replies
                .remove(&reply.message_id)
                .and_then(|slot| slot.pop());
            let pending_len_after_remove = pending_pushlog_replies.len();
            if let Some(sender) = sender {
                let _ = sender.send(reply);
            } else {
                warn!(
                    peer_id = %peer_id,
                    message_id = %reply.message_id,
                    pending_reply_count = pending_len_after_remove,
                    "Received unmatched two-stream reply on response protocol"
                );
            }
        }
        x if x == protocols::STREAM_DOCSYNC => {
            let request: crate::message::DocSyncRequest = protocols::decode(payload)?;
            if event_tx
                .send_leased(
                    TransportEvent::DocSyncRequest {
                        peer_id: peer_id.clone(),
                        request,
                        token: Some(send),
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit DocSyncRequest");
            }
        }
        x if x == protocols::STREAM_BRANCHABLE => {
            let request: crate::message::BranchableSyncRequest = protocols::decode(payload)?;
            if event_tx
                .send_leased(
                    TransportEvent::BranchableSyncRequest {
                        peer_id: peer_id.clone(),
                        request,
                        token: Some(send),
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit BranchableSyncRequest");
            }
        }
        x if x == protocols::STREAM_CAR => {
            debug!(peer_id = %peer_id, "CAR dispatch: reading request");
            let request: crate::message::CarFetchRequest = protocols::decode(payload)?;
            debug!(
                peer_id = %peer_id,
                root_cid = %request.root_cid,
                recursive = request.recursive,
                requested_count = request.wanted_cids.len(),
                "CAR dispatch: emitting CarFetchRequest"
            );
            if event_tx
                .send_leased(
                    TransportEvent::CarFetchRequest {
                        peer_id: peer_id.clone(),
                        request,
                        token: Some(send),
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit CarFetchRequest");
            }
        }
        x if x == protocols::STREAM_CAR_RESP => {
            let car_data: Vec<u8> = protocols::decode(payload)?;
            // Extract the root CID from the CAR headers for event correlation.
            let root_cid = match crate::sync::car::decode_car(&car_data) {
                Ok((roots, _)) => roots.into_iter().next(),
                Err(e) => {
                    warn!("Failed to decode CAR response: {}", e);
                    None
                }
            };
            if let Some(root_cid) = root_cid {
                if event_tx
                    .send_leased(
                        TransportEvent::CarFetchResponse {
                            query_id: None,
                            peer_id: peer_id.clone(),
                            root_cid,
                            car_data,
                        },
                        Box::new(lease),
                    )
                    .await
                    .is_err()
                {
                    warn!("Event channel closed, cannot emit CarFetchResponse");
                }
            }
        }
        x if x == protocols::STREAM_DOCSYNC_RESP => {
            let reply: crate::message::DocSyncReply = protocols::decode(payload)?;
            debug!(peer_id = %peer_id, "Received doc sync response via fire-and-forget");
            if event_tx
                .send_leased(
                    TransportEvent::DocSyncReply {
                        peer_id: peer_id.clone(),
                        reply,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit DocSyncReply");
            }
        }
        x if x == protocols::STREAM_BRANCHABLE_RESP => {
            let reply: crate::message::BranchableSyncReply = protocols::decode(payload)?;
            debug!(peer_id = %peer_id, "Received branchable sync response via fire-and-forget");
            if event_tx
                .send_leased(
                    TransportEvent::BranchableSyncReply {
                        peer_id: peer_id.clone(),
                        reply,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit BranchableSyncReply");
            }
        }
        x if x == protocols::STREAM_SE => {
            let request: crate::message::PushSEArtifactsRequest = protocols::decode(&payload)?;
            verify_iroh_message(&request)?;
            ensure_iroh_signed_sender(peer_id, request.sender_id.as_str())?;
            debug!(
                peer_id = %peer_id,
                collection_id = %request.collection_id,
                artifact_count = request.artifacts.len(),
                "Received SE artifacts"
            );
            drop(request);
            if event_tx
                .send_leased(
                    TransportEvent::SEArtifactsReceived {
                        peer_id: peer_id.clone(),
                        data: payload,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit SEArtifactsReceived");
            }
        }
        x if x == protocols::STREAM_SE_QUERY_REQ => {
            let request: crate::message::QuerySEArtifactsRequest = protocols::decode(payload)?;
            verify_iroh_message(&request)?;
            ensure_iroh_signed_sender(peer_id, request.sender_id.as_str())?;
            debug!(
                peer_id = %peer_id,
                message_id = %request.message_id,
                collection_id = %request.collection_id,
                query_count = request.queries.len(),
                "Received SE query request"
            );
            if event_tx
                .send_leased(
                    TransportEvent::SEQueryRequest {
                        peer_id: peer_id.clone(),
                        request,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit SEQueryRequest");
            }
        }
        x if x == protocols::STREAM_SE_QUERY_RESP => {
            let reply: crate::message::QuerySEArtifactsReply = protocols::decode(payload)?;
            verify_iroh_message(&reply)?;
            ensure_iroh_signed_sender(peer_id, reply.sender_id.as_str())?;
            debug!(
                peer_id = %peer_id,
                message_id = %reply.message_id,
                doc_count = reply.doc_ids.len(),
                "Received SE query response"
            );
            if event_tx
                .send_leased(
                    TransportEvent::SEQueryReply {
                        peer_id: peer_id.clone(),
                        reply,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit SEQueryReply");
            }
        }
        x if x == protocols::STREAM_MANAGE_REQ => {
            let request: crate::message::ManageRequest = protocols::decode(payload)?;
            verify_iroh_message(&request)?;
            ensure_iroh_signed_sender(peer_id, request.sender_id.as_str())?;
            debug!(
                peer_id = %peer_id,
                message_id = %request.message_id,
                "Received manage request"
            );
            if event_tx
                .send_leased(
                    TransportEvent::ManageRequest {
                        peer_id: peer_id.clone(),
                        request,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit ManageRequest");
            }
        }
        x if x == protocols::STREAM_MANAGE_RESP => {
            let reply: crate::message::ManageReply = protocols::decode(payload)?;
            verify_iroh_message(&reply)?;
            ensure_iroh_signed_sender(peer_id, reply.sender_id.as_str())?;
            debug!(
                peer_id = %peer_id,
                message_id = %reply.message_id,
                "Received manage reply"
            );
            if event_tx
                .send_leased(
                    TransportEvent::ManageReply {
                        peer_id: peer_id.clone(),
                        reply,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit ManageReply");
            }
        }
        x if x == protocols::STREAM_MANAGE_QUERY_REQ => {
            let request: crate::message::ManageQueryRequest = protocols::decode(payload)?;
            verify_iroh_message(&request)?;
            ensure_iroh_signed_sender(peer_id, request.sender_id.as_str())?;
            debug!(
                peer_id = %peer_id,
                message_id = %request.message_id,
                "Received manage query request"
            );
            if event_tx
                .send_leased(
                    TransportEvent::ManageQueryRequest {
                        peer_id: peer_id.clone(),
                        request,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit ManageQueryRequest");
            }
        }
        x if x == protocols::STREAM_MANAGE_QUERY_RESP => {
            let reply: crate::message::ManageQueryReply = protocols::decode(payload)?;
            verify_iroh_message(&reply)?;
            ensure_iroh_signed_sender(peer_id, reply.sender_id.as_str())?;
            debug!(
                peer_id = %peer_id,
                message_id = %reply.message_id,
                "Received manage query reply"
            );
            if event_tx
                .send_leased(
                    TransportEvent::ManageQueryReply {
                        peer_id: peer_id.clone(),
                        reply,
                    },
                    Box::new(lease),
                )
                .await
                .is_err()
            {
                warn!("Event channel closed, cannot emit ManageQueryReply");
            }
        }
        _ => {
            debug!("Unknown stream tag: {:?}", String::from_utf8_lossy(tag));
        }
    }
    Ok(())
}

fn ensure_iroh_signed_sender(peer_id: &PeerId, sender_id: &str) -> crate::error::Result<()> {
    if sender_id == peer_id.as_str() {
        Ok(())
    } else {
        Err(crate::error::Error::Transport(format!(
            "transport peer {} did not match signed sender {}",
            peer_id, sender_id
        )))
    }
}

fn verify_iroh_message<M>(msg: &M) -> crate::error::Result<()>
where
    M: Message + serde::Serialize + Clone,
{
    let signature = msg.signature().ok_or(Error::MissingSignature)?;

    let sender_id: iroh::EndpointId = msg
        .sender_id()
        .parse()
        .map_err(|error: iroh::KeyParsingError| Error::InvalidPeerId(error.to_string()))?;

    let pubkey_bytes: [u8; 32] = msg
        .pubkey()
        .try_into()
        .map_err(|_| Error::PublicKeyDecode("expected 32-byte iroh public key".into()))?;
    let pubkey = iroh::PublicKey::from_bytes(&pubkey_bytes)
        .map_err(|error| Error::PublicKeyDecode(error.to_string()))?;

    if pubkey != sender_id {
        return Err(Error::PubkeyPeerIdMismatch);
    }

    let signature = iroh::Signature::try_from(signature).map_err(|_| Error::InvalidSignature)?;
    let mut msg_for_verify = msg.clone();
    msg_for_verify.set_signature(None);
    let bytes = defra_core::cbor::to_vec(&msg_for_verify)
        .map_err(|e| Error::CborSerialization(e.to_string()))?;

    pubkey
        .verify(&bytes, &signature)
        .map_err(|_| Error::InvalidSignature)
}

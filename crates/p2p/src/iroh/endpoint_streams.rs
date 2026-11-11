//! Incoming connection and stream processing for the iroh endpoint.

use std::net::SocketAddr;
use std::sync::Arc;

use futures::future::Either;
use iroh::endpoint::Connection;
use iroh::EndpointId;
use tracing::{debug, warn};

use crate::error::Error;
use crate::message::Message;
use crate::sync::DispatchSender;
use crate::transport::{PeerId, TransportEvent};

use super::endpoint::{spawn_task, EndpointResources, SpawnedTasks};
use super::endpoint_config::PeerAdmission;
use super::endpoint_rpc::{
    close_peer_connections, evict_connection, remember_connection, restore_displaced_connection,
    ConnectionCache, DISCONNECT_ERROR_CODE,
};
use super::inbound_budget::{InboundBudget, InboundLease, PeerShare};
use super::peer_map::{endpoint_id_to_peer_id, SharedPeerMap};
use super::protocol_stream::{ProtocolRecvStream, ProtocolSendStream};
use super::protocols;
use super::records::{Input as RecordsInput, RecordsHandle, RecordsMessage};
use super::services::{self, Service};
use super::solicited::Solicited;
use super::stream_protocols::{InboundStream, ProtocolRequest, Registered, StreamProtocols};

/// QUIC application error code closing a connection refused by the inbound
/// allowlist. Distinct from the disconnect code (0) used elsewhere.
const REFUSED_CONNECTION_CODE: u32 = 1;

/// Whether the remote already closed `connection` with the refusal code. The
/// code is matched, never the reason bytes.
pub(super) fn closed_by_refusal(connection: &iroh::endpoint::Connection) -> bool {
    matches!(
        connection.close_reason(),
        Some(iroh::endpoint::ConnectionError::ApplicationClosed(close))
            if close.error_code == REFUSED_CONNECTION_CODE.into()
    )
}

/// QUIC code a stream is stopped with when no protocol serves its tag, Defra's
/// or an embedder's. An opener sees it as
/// [`Error::StreamReset`](crate::error::Error::StreamReset).
pub const RESET_UNKNOWN_SERVICE: u32 = 2;
/// QUIC code a stream is stopped with when its message is larger than its
/// tag accepts.
pub const RESET_MESSAGE_TOO_LARGE: u32 = 3;
/// QUIC code a stream is stopped with when no room in the peer's share of the
/// budget frees up for its message in time.
pub const RESET_BUSY: u32 = 4;
/// QUIC code a stream is reset with when its message body stops arriving.
pub const RESET_STALLED: u32 = 5;
/// QUIC code a stream is stopped with when it carries a reply nobody asked
/// this peer for.
const RESET_UNSOLICITED: u32 = 6;

/// Handle an incoming QUIC connection.
pub(super) async fn handle_incoming(
    incoming: iroh::endpoint::Incoming,
    resources: &EndpointResources,
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

    let lifecycle = ConnectionLifecycle::new(resources, event_tx.clone());
    let _ = adopt_new_connection(
        &lifecycle,
        &resources.connection_cache,
        connection,
        remote_addr,
        ConnectionDirection::Inbound,
    )
    .await;
}

/// Shared state for every stream accepted on one authenticated connection.
#[derive(Clone)]
pub(super) struct ConnectionStreamContext {
    event_tx: DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
    peer_map: Arc<SharedPeerMap>,
    connection_cache: ConnectionCache,
    node_identity: Option<Arc<identity::RawIdentity>>,
    spawned_tasks: SpawnedTasks,
    inbound_budget: Arc<InboundBudget>,
    share: Arc<PeerShare>,
    records: RecordsHandle,
    solicited: Arc<Solicited>,
    stream_protocols: StreamProtocols,
}

/// What adopting a connection needs besides the cache it is published to.
#[derive(Clone)]
pub(super) struct ConnectionLifecycle {
    event_tx: DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
    peer_map: Arc<SharedPeerMap>,
    node_identity: Option<Arc<identity::RawIdentity>>,
    spawned_tasks: SpawnedTasks,
    admission: Arc<PeerAdmission>,
    inbound_budget: Arc<InboundBudget>,
    records: RecordsHandle,
    solicited: Arc<Solicited>,
    stream_protocols: StreamProtocols,
}

impl ConnectionLifecycle {
    pub(super) fn new(
        resources: &EndpointResources,
        event_tx: DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
    ) -> Self {
        Self {
            event_tx,
            peer_map: Arc::clone(&resources.peer_map),
            node_identity: resources.node_identity.clone(),
            spawned_tasks: Arc::clone(&resources.spawned_tasks),
            admission: Arc::clone(&resources.admission),
            inbound_budget: Arc::clone(&resources.inbound_budget),
            records: resources.records.clone(),
            solicited: Arc::clone(&resources.solicited),
            stream_protocols: resources.stream_protocols.clone(),
        }
    }

    fn stream_context(
        &self,
        remote_id: EndpointId,
        cache: &ConnectionCache,
    ) -> ConnectionStreamContext {
        ConnectionStreamContext {
            event_tx: self.event_tx.clone(),
            peer_map: Arc::clone(&self.peer_map),
            connection_cache: Arc::clone(cache),
            node_identity: self.node_identity.clone(),
            spawned_tasks: Arc::clone(&self.spawned_tasks),
            inbound_budget: Arc::clone(&self.inbound_budget),
            share: self.inbound_budget.peer_share(remote_id),
            records: self.records.clone(),
            solicited: Arc::clone(&self.solicited),
            stream_protocols: self.stream_protocols.clone(),
        }
    }
}

/// Which side created a connection, deciding its admission check and refusal.
#[derive(Clone, Copy)]
pub(super) enum ConnectionDirection {
    Inbound,
    Outbound,
}

/// Adopt a connection this endpoint just accepted or dialed: count it, cache
/// it, recheck admission and hand it to a pump that serves its streams and
/// announces it. A cache hit is already adopted; adopting it again would
/// double its count.
pub(super) async fn adopt_new_connection(
    lifecycle: &ConnectionLifecycle,
    cache: &ConnectionCache,
    connection: Connection,
    remote_addr: Option<SocketAddr>,
    direction: ConnectionDirection,
) -> crate::error::Result<()> {
    let remote_id = connection.remote_id();
    let peer_id = endpoint_id_to_peer_id(&remote_id);
    let is_new =
        lifecycle
            .peer_map
            .increment_connections(remote_id, remote_addr, connection.clone());
    let displaced = remember_connection(cache, &connection);

    // Publish before rechecking: a racing revoke either closes the published
    // handles or is observed here.
    let (admitted, code, reason) = match direction {
        ConnectionDirection::Inbound => (
            lifecycle.admission.admits_inbound(&remote_id),
            REFUSED_CONNECTION_CODE,
            &b"peer not authorized"[..],
        ),
        ConnectionDirection::Outbound => (
            lifecycle.admission.admits_outbound(&remote_id),
            DISCONNECT_ERROR_CODE,
            &b"peer revoked"[..],
        ),
    };
    let revoked = !admitted && lifecycle.admission.is_revoked(&remote_id);
    if !admitted {
        warn!(peer_id = %peer_id, "Closing Iroh connection: peer was refused while it was adopted");
        // Close with the typed code first: QUIC keeps the first close, and the
        // peer-wide close below would otherwise send the disconnect code.
        connection.close(code.into(), reason);
        // The allowlist only governs connections to us, so a miss spares the
        // sibling this node dialed; a revoke bars the peer outright.
        if revoked {
            close_peer_connections(&lifecycle.peer_map, cache, &remote_id);
        } else {
            restore_displaced_connection(cache, &connection, displaced);
        }
    }

    // The pump owns the count from here, and with it the `PeerConnected`
    // send, which can park on a full lane: the adopt has no await left to be
    // cancelled at (`ConnectionAdoption.tla` PumpAnnounce).
    let context = lifecycle.stream_context(remote_id, cache);
    let announce = is_new && admitted;
    let pump = spawn_task(&lifecycle.spawned_tasks, {
        let (connection, context) = (connection.clone(), context.clone());
        async move { handle_connection_streams(connection, remote_id, context, announce).await }
    });
    if pump.is_none() {
        connection.close(DISCONNECT_ERROR_CODE.into(), b"endpoint shutting down");
        finish_connection(&connection, remote_id, &context).await;
        return Err(Error::Dial(format!(
            "connection to {peer_id} raced endpoint shutdown"
        )));
    }
    if !admitted {
        return Err(Error::Dial(match (direction, revoked) {
            (ConnectionDirection::Outbound, _) => {
                format!("dial to {peer_id} raced a revoke: connection closed")
            }
            (ConnectionDirection::Inbound, true) => {
                format!("inbound connection from revoked peer {peer_id} refused")
            }
            (ConnectionDirection::Inbound, false) => {
                format!("inbound connection from {peer_id} refused by the allowlist")
            }
        }));
    }

    cache.gossip.connected(remote_id, &connection);
    Ok(())
}

/// Process streams on an accepted connection, dispatching by stream tag.
///
/// The tag is read inside the spawned task, not the accept loop, so a peer that
/// stalls before writing one cannot hold up the other protocols.
///
/// Emits `PeerConnected` when `announce` (the peer's first admitted
/// connection), while streams are already served, and `PeerDisconnected` only
/// when the last connection for this peer closes, always after its own
/// `PeerConnected`.
pub(super) async fn handle_connection_streams(
    connection: Connection,
    remote_id: EndpointId,
    context: ConnectionStreamContext,
    announce: bool,
) {
    let peer_id = endpoint_id_to_peer_id(&remote_id);
    let announcing = async {
        if announce
            && context
                .event_tx
                .send(TransportEvent::PeerConnected(peer_id.clone()))
                .await
                .is_err()
        {
            warn!("Event channel closed, cannot emit PeerConnected");
        }
    };
    futures::join!(
        announcing,
        serve_connection_streams(&connection, remote_id, &peer_id, &context)
    );

    finish_connection(&connection, remote_id, &context).await;
}

/// Accept and dispatch the streams the peer opens until the connection closes.
async fn serve_connection_streams(
    connection: &Connection,
    remote_id: EndpointId,
    peer_id: &PeerId,
    context: &ConnectionStreamContext,
) {
    // On mux 0.2 every reply goes back on its request's stream.
    let replies_on_stream = connection.alpn() == protocols::ALPN_MUX;

    while let Ok((send, mut recv)) = connection.accept_bi().await {
        let peer_id = peer_id.clone();
        let event_tx = context.event_tx.clone();
        let node_identity = context.node_identity.clone();
        let inbound_budget = Arc::clone(&context.inbound_budget);
        let share = Arc::clone(&context.share);
        let records = context.records.clone();
        let solicited = Arc::clone(&context.solicited);
        let stream_protocols = context.stream_protocols.clone();
        let connection = connection.clone();
        let _ = spawn_task(&context.spawned_tasks, async move {
            let admission = Admission {
                budget: &inbound_budget,
                share: &share,
                solicited: &solicited,
                stream_protocols: &stream_protocols,
            };
            let Some((service, payload, lease)) = admit_stream(
                &mut recv,
                &admission,
                replies_on_stream,
                remote_id,
                &peer_id,
            )
            .await
            else {
                return;
            };
            // Replies on this stream go out at the service's priority.
            let _ = send.set_priority(service.priority.send_priority());
            if let Some(registered) = stream_protocols.get(service.tag) {
                // The room goes with the request: back when the handler drops it.
                let request = ProtocolRequest::new(payload, lease);
                serve_protocol_stream(registered, &connection, peer_id, request, send, recv).await;
                return;
            }
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
                node_identity.as_deref(),
                replies_on_stream,
            )
            .await
            {
                debug!("Stream error from {}: {}", peer_id, e);
            }
        });
    }
}

/// Release one adopted connection: evict its cache entry, drop its count and
/// announce the peer gone with its last connection.
async fn finish_connection(
    connection: &Connection,
    remote_id: EndpointId,
    context: &ConnectionStreamContext,
) {
    evict_connection(&context.connection_cache, remote_id, connection);
    let fully_disconnected = context.peer_map.decrement_connections(&remote_id);
    let peer_id = endpoint_id_to_peer_id(&remote_id);
    debug!(peer_id = %peer_id, fully_disconnected, "Connection closed");
    if fully_disconnected {
        crate::model_trace!(Disconnect {
            peer: peer_id.to_string(),
        });
    }

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

/// Serve an admitted stream of an embedder's protocol on this task until its
/// handler returns, the protocol's deadline passes or the connection closes,
/// whichever comes first. The last two drop the handler, which resets both
/// halves and frees the request's room; shutdown aborts the task, with the
/// same effect.
async fn serve_protocol_stream(
    registered: &Registered,
    connection: &Connection,
    peer: PeerId,
    request: ProtocolRequest,
    send: iroh::endpoint::SendStream,
    recv: iroh::endpoint::RecvStream,
) {
    let deadline = registered.protocol.deadline;
    let stream = InboundStream {
        peer: peer.clone(),
        request,
        deadline: web_time::Instant::now() + deadline,
        send: ProtocolSendStream::new(send),
        recv: ProtocolRecvStream::new(recv),
    };
    let closed = std::pin::pin!(connection.closed());
    let served = std::pin::pin!(crate::rt::time::timeout(
        deadline,
        registered.handler.handle(stream)
    ));
    let tag = String::from_utf8_lossy(registered.protocol.tag);
    match futures::future::select(closed, served).await {
        Either::Left(_) => {
            debug!(peer_id = %peer, %tag, "Connection closed: protocol stream cancelled");
        }
        Either::Right((Err(_), _)) => {
            debug!(peer_id = %peer, %tag, ?deadline, "Protocol stream reached its deadline");
        }
        Either::Right((Ok(()), _)) => {}
    }
}

/// What a stream is admitted under, shared by every stream on one connection.
pub(super) struct Admission<'a> {
    pub(super) budget: &'a InboundBudget,
    pub(super) share: &'a Arc<PeerShare>,
    pub(super) solicited: &'a Solicited,
    pub(super) stream_protocols: &'a StreamProtocols,
}

/// Read a stream's tag and length, then wait for room for its body. Resets
/// the stream instead when the tag is neither Defra's nor a registered
/// protocol's, is a reply stream on a connection whose replies use their
/// request's stream, is a reply nobody asked this peer for, the message is
/// too large for its tag, or no room frees up before the deadline.
pub(super) async fn admit_stream<'a>(
    recv: &mut iroh::endpoint::RecvStream,
    admission: &Admission<'a>,
    replies_on_stream: bool,
    remote_id: EndpointId,
    peer_id: &PeerId,
) -> Option<(&'a Service, Vec<u8>, InboundLease)> {
    let &Admission {
        budget,
        share,
        solicited,
        stream_protocols,
    } = admission;
    let tag = protocols::read_stream_tag(recv)
        .await
        .inspect_err(|e| debug!(peer_id = %peer_id, error = %e, "Unreadable stream tag"))
        .ok()?;
    let service = services::lookup(&tag).or_else(|| {
        stream_protocols
            .get(&tag)
            .map(|registered| &registered.service)
    });
    let Some(service) = service else {
        debug!(peer_id = %peer_id, tag = %String::from_utf8_lossy(&tag), "Unknown stream tag");
        let _ = recv.stop(RESET_UNKNOWN_SERVICE.into());
        return None;
    };
    if service.solicited && (replies_on_stream || !solicited.take(remote_id, service.tag)) {
        debug!(peer_id = %peer_id, tag = %String::from_utf8_lossy(service.tag), "Reply stream with no request outstanding");
        let _ = recv.stop(RESET_UNSOLICITED.into());
        return None;
    }
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
    let deadlines = budget.deadlines();
    let Ok(lease) = budget
        .admit(share, service.priority, len, deadlines.admit)
        .await
    else {
        debug!(peer_id = %peer_id, len, class = service.priority.name(), "No inbound room before the deadline");
        let _ = recv.stop(RESET_BUSY.into());
        return None;
    };
    // The idle deadline catches a sender that stops; the total deadline catches
    // one that trickles bytes to keep its lease.
    let payload = crate::rt::time::timeout(
        deadlines.body(len),
        protocols::read_body_until_idle(recv, len, deadlines.idle),
    )
    .await
    .unwrap_or(Ok(None))
    .inspect_err(|e| debug!(peer_id = %peer_id, error = %e, "Unreadable message body"))
    .ok()?;
    let Some(payload) = payload else {
        debug!(peer_id = %peer_id, len, "Message body stopped arriving");
        // Free the room before the sender can see the reset.
        drop(lease);
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
    node_identity: Option<&identity::RawIdentity>,
    replies_on_stream: bool,
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
                        token: replies_on_stream.then_some(send),
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
                        token: replies_on_stream.then_some(send),
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
                        token: replies_on_stream.then_some(send),
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

pub(super) fn ensure_iroh_signed_sender(
    peer_id: &PeerId,
    sender_id: &str,
) -> crate::error::Result<()> {
    if sender_id == peer_id.as_str() {
        Ok(())
    } else {
        Err(crate::error::Error::Transport(format!(
            "transport peer {} did not match signed sender {}",
            peer_id, sender_id
        )))
    }
}

pub(super) fn verify_iroh_message<M>(msg: &M) -> crate::error::Result<()>
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

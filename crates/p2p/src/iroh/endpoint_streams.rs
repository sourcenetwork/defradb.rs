//! Incoming connection and stream processing for the iroh endpoint.

use std::net::SocketAddr;
use std::sync::Arc;

use iroh::endpoint::Connection;
use iroh::EndpointId;
use tracing::{debug, warn};

use crate::error::Error;
use crate::message::Message;
use crate::sync::DispatchSender;
use crate::transport::{PeerId, TransportEvent};

use super::endpoint::{spawn_task, EndpointResources, SpawnedTasks};
use super::endpoint_config::PeerAdmission;
use super::endpoint_rpc::{ConnectionCache, DISCONNECT_ERROR_CODE};
use super::gossip::GossipHandle;
use super::inbound_budget::{InboundBudget, InboundLease, PeerShare};
use super::peer_map::{endpoint_id_to_peer_id, SharedPeerMap};
use super::protocols;
use super::records::{Input as RecordsInput, RecordsHandle, RecordsMessage};
use super::services::{self, Service};
use super::solicited::Solicited;

/// QUIC application error code closing a connection refused by the inbound
/// allowlist. Distinct from the disconnect code (0) used elsewhere.
const REFUSED_CONNECTION_CODE: u32 = 1;

/// QUIC stream error codes sent when a stream is refused before its body is
/// read, or reset because the body stopped arriving.
const RESET_UNKNOWN_SERVICE: u32 = 2;
pub(super) const RESET_MESSAGE_TOO_LARGE: u32 = 3;
const RESET_BUSY: u32 = 4;
pub(super) const RESET_STALLED: u32 = 5;
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
    adopt_new_connection(
        &lifecycle,
        &resources.connection_cache,
        connection,
        remote_id,
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
}

/// Everything a newly created connection needs in order to be adopted:
/// published to both stores, admitted, announced, and pumped.
///
/// Cloneable and free of `EndpointResources`, so the RPC dial path can hold it
/// on the connection cache without an ownership cycle.
#[derive(Clone)]
pub(super) struct ConnectionLifecycle {
    event_tx: DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
    peer_map: Arc<SharedPeerMap>,
    node_identity: Option<Arc<identity::RawIdentity>>,
    spawned_tasks: SpawnedTasks,
    admission: Arc<PeerAdmission>,
    gossip: GossipHandle,
    inbound_budget: Arc<InboundBudget>,
    records: RecordsHandle,
    solicited: Arc<Solicited>,
}

impl ConnectionLifecycle {
    pub(super) fn new(
        resources: &EndpointResources,
        event_tx: DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
    ) -> Self {
        Self::from_parts(
            event_tx,
            Arc::clone(&resources.peer_map),
            resources.node_identity.clone(),
            Arc::clone(&resources.spawned_tasks),
            Arc::clone(&resources.admission),
            resources.gossip.clone(),
            Arc::clone(&resources.inbound_budget),
            resources.records.clone(),
            Arc::clone(&resources.solicited),
        )
    }

    /// The same handles, taken individually, so the event loop can build the
    /// lifecycle before the resources that hold the cache it goes on.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_parts(
        event_tx: DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
        peer_map: Arc<SharedPeerMap>,
        node_identity: Option<Arc<identity::RawIdentity>>,
        spawned_tasks: SpawnedTasks,
        admission: Arc<PeerAdmission>,
        gossip: GossipHandle,
        inbound_budget: Arc<InboundBudget>,
        records: RecordsHandle,
        solicited: Arc<Solicited>,
    ) -> Self {
        Self {
            event_tx,
            peer_map,
            node_identity,
            spawned_tasks,
            admission,
            gossip,
            inbound_budget,
            records,
            solicited,
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
            connection_cache: cache.clone(),
            node_identity: self.node_identity.clone(),
            spawned_tasks: Arc::clone(&self.spawned_tasks),
            inbound_budget: Arc::clone(&self.inbound_budget),
            share: self.inbound_budget.peer_share(remote_id),
            records: self.records.clone(),
            solicited: Arc::clone(&self.solicited),
        }
    }
}

/// Which side of the endpoint created a connection. It decides the admission
/// check and the close code a refused connection carries.
#[derive(Clone, Copy)]
pub(super) enum ConnectionDirection {
    Inbound,
    Outbound,
}

impl ConnectionDirection {
    fn admits(self, admission: &PeerAdmission, id: &EndpointId) -> bool {
        match self {
            Self::Inbound => admission.admits_inbound(id),
            Self::Outbound => admission.admits_outbound(id),
        }
    }

    fn refusal_code(self) -> u32 {
        match self {
            Self::Inbound => REFUSED_CONNECTION_CODE,
            Self::Outbound => DISCONNECT_ERROR_CODE,
        }
    }

    fn refusal_reason(self) -> &'static [u8] {
        match self {
            Self::Inbound => b"peer not authorized",
            Self::Outbound => b"peer revoked",
        }
    }
}

/// What became of a connection offered to [`adopt_new_connection`].
///
/// The two refusals are different events and callers report them differently:
/// a peer that is not admitted is a revoke or an allowlist miss, while a closed
/// task registry is this endpoint shutting down.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum AdoptionOutcome {
    /// Published, admitted, announced and pumped.
    Admitted,
    /// The peer is not admitted: revoked, or an inbound allowlist miss.
    Refused,
    /// The task registry is closed, so nothing could pump the connection.
    ShuttingDown,
}

/// Adopt a connection this process just created: the one place a new mux is
/// published, admitted, announced and pumped.
///
/// Call it only for a connection this process created or accepted. A cache hit
/// is already adopted, and adopting it again would double the peer-map count
/// and split `accept_bi` between two pumps.
///
/// A refused connection is closed and still pumped, so the count this call
/// added is always decremented.
pub(super) async fn adopt_new_connection(
    lifecycle: &ConnectionLifecycle,
    cache: &ConnectionCache,
    connection: Connection,
    remote_id: EndpointId,
    remote_addr: Option<SocketAddr>,
    direction: ConnectionDirection,
) -> AdoptionOutcome {
    let peer_id = endpoint_id_to_peer_id(&remote_id);

    // Publish to both stores before re-checking admission. A revoke either
    // closes the published handles or is observed here after a late
    // publication, so one of the two always sees the other.
    //
    // The publish displaces whatever the cache held for this peer, so keep it:
    // a refusal that is not a revoke has to put a live sibling back.
    let displaced = super::endpoint_rpc::displaced_connection(cache, &connection);
    let is_new =
        lifecycle
            .peer_map
            .increment_connections(remote_id, remote_addr, connection.clone());
    super::endpoint_rpc::remember_connection(cache, &connection);

    let admitted = direction.admits(&lifecycle.admission, &remote_id);
    if !admitted {
        // A revoke bars the peer outright, so every connection it holds goes.
        // An inbound allowlist miss is narrower: the allowlist only governs who
        // may open a connection TO us, so a sibling outbound session this node
        // dialled stays legal and must survive. Closing this handle is enough
        // for clones of this QUIC connection; the pump evicts the cache entry
        // published above.
        let revoked = lifecycle.admission.is_revoked(&remote_id);
        warn!(
            peer_id = %peer_id,
            revoked,
            "Closing Iroh connection: peer was not admitted while it was being established"
        );
        if revoked {
            super::endpoint_rpc::close_peer_connections(&lifecycle.peer_map, cache, &remote_id);
        } else {
            super::endpoint_rpc::restore_displaced_connection(cache, &connection, displaced);
        }
        connection.close(direction.refusal_code().into(), direction.refusal_reason());
    }

    // Always pump, including a refused connection: `increment_connections`
    // above has already counted it, and the pump's cleanup is what decrements
    // it. Returning early would leave the count stuck above zero, so the
    // revoked peer would sit in `connected_peers` forever.
    //
    // Registered before any await below, deliberately. `PeerConnected` is a
    // completion-class event, so its `send` can park on a full lane; a
    // cancelled adopt parked there would leave a counted, cached connection
    // with no pump and nothing to undo the publish. Registering first means the
    // pump owns the cleanup from the moment the connection is published.
    let context = lifecycle.stream_context(remote_id, cache);
    let pump_connection = connection.clone();
    let pump_context = context.clone();
    let pump = spawn_task(&lifecycle.spawned_tasks, async move {
        handle_connection_streams(pump_connection, remote_id, pump_context).await;
    });
    if pump.is_none() {
        // The registry is closed, so the future was never polled and no pump
        // owns this connection. Close our own handle — only this one, never
        // every connection the peer holds, which may be healthy — and then run
        // the pump's exit path once so the count this call added is balanced.
        // The connection is not usable, so this is not an admission.
        connection.close(direction.refusal_code().into(), b"endpoint shutting down");
        finish_connection(&connection, remote_id, &context).await;
        return AdoptionOutcome::ShuttingDown;
    }

    if admitted {
        lifecycle.gossip.connected(remote_id, &connection);
    }

    if admitted
        && is_new
        && lifecycle
            .event_tx
            .send(TransportEvent::PeerConnected(peer_id.clone()))
            .await
            .is_err()
    {
        warn!("Event channel closed, cannot emit PeerConnected");
    }

    if admitted {
        AdoptionOutcome::Admitted
    } else {
        AdoptionOutcome::Refused
    }
}

/// The one exit path for a connection: drop this handle from the cache,
/// decrement the peer-map count, and announce the peer gone when this was its
/// last connection.
///
/// Called by the pump when `accept_bi` ends, and directly by
/// [`adopt_new_connection`] when the task registry refused the pump. Exactly
/// one of the two runs per adopted connection.
async fn finish_connection(
    connection: &Connection,
    remote_id: EndpointId,
    context: &ConnectionStreamContext,
) {
    super::endpoint_rpc::evict_connection(&context.connection_cache, remote_id, connection);
    let fully_disconnected = context.peer_map.decrement_connections(&remote_id);
    debug!(
        peer_id = %endpoint_id_to_peer_id(&remote_id),
        fully_disconnected,
        "Connection closed"
    );

    if fully_disconnected
        && context
            .event_tx
            .send(TransportEvent::PeerDisconnected(endpoint_id_to_peer_id(
                &remote_id,
            )))
            .await
            .is_err()
    {
        debug!("Event channel closed, cannot emit PeerDisconnected");
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
        let _ = spawn_task(&context.spawned_tasks, async move {
            let Some((service, payload, lease)) = admit_stream(
                &mut recv,
                &inbound_budget,
                &share,
                &solicited,
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

    finish_connection(&connection, remote_id, &context).await;
}

/// Read a stream's tag and length, then wait for room for its body. Resets
/// the stream instead when the tag is unknown, is a reply stream on a
/// connection whose replies use their request's stream, is a reply nobody
/// asked this peer for, the message is too large for its tag, or no room
/// frees up before the deadline.
pub(super) async fn admit_stream(
    recv: &mut iroh::endpoint::RecvStream,
    budget: &InboundBudget,
    share: &Arc<PeerShare>,
    solicited: &Solicited,
    replies_on_stream: bool,
    remote_id: EndpointId,
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
    let payload = n0_future::time::timeout(
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

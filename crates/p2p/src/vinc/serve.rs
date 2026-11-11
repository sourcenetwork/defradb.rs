//! The streams a peer opens: read under admission, then answered here or
//! handed to the engine with a token for the reply.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, warn};
use vinc::{Connection, Hosted, NodeId, RecvStream, SendStream};

use crate::error::{Error, Result};
use crate::message::Message;
use crate::transport::{PeerId, TransportEvent};

use super::addr::node_to_peer_id;
use super::command::ReplyBody;
use super::endpoint::{Node, REPLY_DEADLINE};
use super::inbound_budget::{InboundLease, PeerShare};
use super::services::{self, Service};
use super::token::VincResponseToken;
use super::wire;

/// Stream reset codes, as on iroh.
pub const RESET_UNKNOWN_SERVICE: u64 = 2;
pub const RESET_MESSAGE_TOO_LARGE: u64 = 3;
pub const RESET_BUSY: u64 = 4;
pub const RESET_STALLED: u64 = 5;
/// The engine dropped its token without replying.
pub const RESET_DROPPED: u64 = 7;

/// How long a reply may take to write before its stream is reset.
const REPLY_WRITE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

impl<H: Hosted + 'static> Node<H> {
    /// Serve every stream `peer` opens on `connection` until it ends, then
    /// forget the connection.
    pub(super) async fn serve_connection(self: Rc<Self>, connection: Connection<H>, serial: u64) {
        let peer = connection.remote();
        let share = self.budget.peer_share(peer);
        // The tag is read in the stream's own task, not here, so a peer that
        // stalls before writing one cannot hold up its other streams.
        while let Ok((send, recv)) = connection.accept_bi().await {
            let server = Rc::clone(&self);
            let share = Arc::clone(&share);
            let spawned = self.spawn(async move {
                server.serve_stream(peer, serial, &share, send, recv).await;
            });
            if spawned.is_none() {
                debug!(peer_id = %node_to_peer_id(&peer), "vinc stream refused: no room for its task");
            }
        }
        debug!(peer_id = %node_to_peer_id(&peer), serial, "vinc connection ended");
        self.disconnected(&peer, serial).await;
    }

    async fn serve_stream(
        self: Rc<Self>,
        peer: NodeId,
        serial: u64,
        share: &Arc<PeerShare>,
        mut send: SendStream<H>,
        mut recv: RecvStream<H>,
    ) {
        let peer_id = node_to_peer_id(&peer);
        let idle = self.budget.deadlines().idle;
        let tag = match crate::rt::time::timeout(idle, vinc::read_tag(&mut recv)).await {
            Ok(Ok(tag)) => tag,
            Ok(Err(error)) => {
                debug!(%peer_id, %error, "unreadable vinc stream tag");
                recv.stop(RESET_UNKNOWN_SERVICE).await;
                return;
            }
            Err(_) => {
                recv.stop(RESET_STALLED).await;
                return;
            }
        };
        let Some((service, payload, lease)) = self.admit(&peer_id, share, &tag, &mut recv).await
        else {
            return;
        };
        // The reply leaves at the class of its request.
        let _ = send.set_urgency(service.priority.urgency()).await;
        if let Err(error) = self
            .dispatch(service, &peer, &peer_id, serial, send, payload, lease)
            .await
        {
            debug!(%peer_id, tag = %String::from_utf8_lossy(service.tag), %error, "vinc stream error");
        }
    }

    /// Read a stream's length, then wait for room for its body. Stops the
    /// stream instead when the tag is unknown, the message is too large for
    /// it, or no room frees up in time.
    async fn admit(
        &self,
        peer_id: &PeerId,
        share: &Arc<PeerShare>,
        tag: &[u8],
        recv: &mut RecvStream<H>,
    ) -> Option<(&'static Service, Vec<u8>, InboundLease)> {
        let Some(service) = services::lookup(tag) else {
            debug!(%peer_id, tag = %String::from_utf8_lossy(tag), "unknown vinc stream tag");
            recv.stop(RESET_UNKNOWN_SERVICE).await;
            return None;
        };
        let len = wire::read_length(recv)
            .await
            .inspect_err(|e| debug!(%peer_id, error = %e, "unreadable message length"))
            .ok()?;
        if len > service.max_message {
            self.budget.record_oversized(service.priority);
            recv.stop(RESET_MESSAGE_TOO_LARGE).await;
            return None;
        }
        let deadlines = self.budget.deadlines();
        let Ok(lease) = self
            .budget
            .admit(share, service.priority, len, deadlines.admit)
            .await
        else {
            recv.stop(RESET_BUSY).await;
            return None;
        };
        let payload = crate::rt::time::timeout(
            deadlines.body(len),
            wire::read_body_until_idle(recv, len, deadlines.idle),
        )
        .await
        .unwrap_or(Ok(None))
        .inspect_err(|e| debug!(%peer_id, error = %e, "unreadable message body"))
        .ok()?;
        let Some(payload) = payload else {
            drop(lease);
            recv.stop(RESET_STALLED).await;
            return None;
        };
        Some((service, payload, lease))
    }

    /// Hold `send` for the engine's reply under a fresh token, or refuse the
    /// stream busy when too many replies are owed.
    async fn token(&self, send: SendStream<H>, serial: u64) -> Option<VincResponseToken> {
        match self
            .replies
            .issue(send, serial, Instant::now() + REPLY_DEADLINE)
        {
            Ok(id) => Some(VincResponseToken::new(id, self.commands.clone())),
            Err(mut send) => {
                warn!("vinc reply table is full; refusing the request busy");
                send.reset(RESET_BUSY).await;
                None
            }
        }
    }

    async fn emit(&self, event: TransportEvent<VincResponseToken>, lease: InboundLease) {
        let kind = event.kind();
        if self
            .events
            .send_leased(event, Box::new(lease))
            .await
            .is_err()
        {
            warn!(kind, "Event channel closed, cannot emit a vinc request");
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn dispatch(
        &self,
        service: &'static Service,
        peer: &NodeId,
        peer_id: &PeerId,
        serial: u64,
        mut send: SendStream<H>,
        payload: Vec<u8>,
        lease: InboundLease,
    ) -> Result<()> {
        match service.tag {
            services::STREAM_IDENTITY => {
                let request: crate::message::IdentityRequest = wire::decode(payload)?;
                let response = self.identity_response(&request, peer_id);
                write_reply(send, ReplyBody::Framed(wire::encode(&response)?)).await?;
            }
            services::STREAM_RECORD => {
                self.directory.insert(*peer, payload);
                drop(lease);
                let _ = send.finish().await;
            }
            services::STREAM_PUSHLOG => {
                let request = wire::decode(payload)?;
                if let Some(token) = self.token(send, serial).await {
                    self.emit(
                        TransportEvent::PushLogRequest {
                            peer_id: peer_id.clone(),
                            request,
                            token,
                        },
                        lease,
                    )
                    .await;
                }
            }
            services::STREAM_TWOSTREAM => {
                let request = wire::decode(payload)?;
                if let Some(token) = self.token(send, serial).await {
                    self.emit(
                        TransportEvent::TwoStreamRequest {
                            peer_id: peer_id.clone(),
                            request,
                            token: Some(token),
                            is_explicit_replicator: false,
                            explicit_replay_authorization: None,
                        },
                        lease,
                    )
                    .await;
                }
            }
            services::STREAM_DOCSYNC => {
                let request = wire::decode(payload)?;
                if let Some(token) = self.token(send, serial).await {
                    self.emit(
                        TransportEvent::DocSyncRequest {
                            peer_id: peer_id.clone(),
                            request,
                            token: Some(token),
                        },
                        lease,
                    )
                    .await;
                }
            }
            services::STREAM_BRANCHABLE => {
                let request = wire::decode(payload)?;
                if let Some(token) = self.token(send, serial).await {
                    self.emit(
                        TransportEvent::BranchableSyncRequest {
                            peer_id: peer_id.clone(),
                            request,
                            token: Some(token),
                        },
                        lease,
                    )
                    .await;
                }
            }
            services::STREAM_CAR => {
                let request = wire::decode(payload)?;
                if let Some(token) = self.token(send, serial).await {
                    self.emit(
                        TransportEvent::CarFetchRequest {
                            peer_id: peer_id.clone(),
                            request,
                            token: Some(token),
                        },
                        lease,
                    )
                    .await;
                }
            }
            services::STREAM_SE => {
                let request: crate::message::PushSEArtifactsRequest = wire::decode(&payload)?;
                verify_signed_sender(peer_id, &request)?;
                drop(request);
                // The sender waits for this end before it lets go.
                let _ = send.finish().await;
                self.emit(
                    TransportEvent::SEArtifactsReceived {
                        peer_id: peer_id.clone(),
                        data: payload,
                    },
                    lease,
                )
                .await;
            }
            services::STREAM_SE_QUERY => {
                let request: crate::message::QuerySEArtifactsRequest = wire::decode(payload)?;
                verify_signed_sender(peer_id, &request)?;
                if let Some(token) = self.token(send, serial).await {
                    self.emit(
                        TransportEvent::SEQueryRequest {
                            peer_id: peer_id.clone(),
                            request,
                            token: Some(token),
                        },
                        lease,
                    )
                    .await;
                }
            }
            services::STREAM_MANAGE => {
                let request: crate::message::ManageRequest = wire::decode(payload)?;
                verify_signed_sender(peer_id, &request)?;
                if let Some(token) = self.token(send, serial).await {
                    self.emit(
                        TransportEvent::ManageRequest {
                            peer_id: peer_id.clone(),
                            request,
                            token: Some(token),
                        },
                        lease,
                    )
                    .await;
                }
            }
            services::STREAM_MANAGE_QUERY => {
                let request: crate::message::ManageQueryRequest = wire::decode(payload)?;
                verify_signed_sender(peer_id, &request)?;
                if let Some(token) = self.token(send, serial).await {
                    self.emit(
                        TransportEvent::ManageQueryRequest {
                            peer_id: peer_id.clone(),
                            request,
                            token: Some(token),
                        },
                        lease,
                    )
                    .await;
                }
            }
            _ => {
                send.reset(RESET_UNKNOWN_SERVICE).await;
            }
        }
        Ok(())
    }

    fn identity_response(
        &self,
        request: &crate::message::IdentityRequest,
        peer_id: &PeerId,
    ) -> crate::message::IdentityResponse {
        if request.peer_id != peer_id.as_str() {
            return crate::message::IdentityResponse::error(
                &request.message_id,
                "identity request peer did not match authenticated endpoint",
            );
        }
        let Some(node_identity) = self.identity.as_deref() else {
            return crate::message::IdentityResponse::identity_unconfigured(&request.message_id);
        };
        match identity::new_token(
            node_identity,
            std::time::Duration::from_secs(60 * 60 * 24),
            Some(peer_id.to_string()),
            None,
        ) {
            Ok(token) => crate::message::IdentityResponse::success(&request.message_id, token),
            Err(error) => crate::message::IdentityResponse::error(
                &request.message_id,
                &format!("failed to generate identity token: {error}"),
            ),
        }
    }

    /// Give the peer on `connection` this node's signed record, so it can
    /// dial this node by id later and read its service-record pointer.
    pub(super) async fn send_record(&self, connection: &Connection<H>) {
        let record = match self.ep.signed_record().await {
            Ok(record) => record,
            Err(error) => {
                debug!(%error, "vinc record could not be signed");
                return;
            }
        };
        let sent = async {
            let (mut send, mut recv) = connection
                .open_bi_tagged(services::STREAM_RECORD)
                .await
                .map_err(|e| wire::vinc_error("open record stream", e))?;
            send.set_urgency(services::urgency_of(services::STREAM_RECORD))
                .await
                .map_err(|e| wire::vinc_error("record urgency", e))?;
            wire::write_payload(&mut send, &record).await?;
            send.finish()
                .await
                .map_err(|e| wire::vinc_error("finish record", e))?;
            let _ =
                crate::rt::time::timeout(std::time::Duration::from_secs(5), recv.read_to_end(16))
                    .await;
            Ok::<_, Error>(())
        };
        if let Err(error) = sent.await {
            debug!(%error, "vinc record not delivered");
        }
    }
}

/// Write `body` on `send` and end the stream. A requester that stops reading
/// gets the stream reset once the write deadline passes.
pub(super) async fn write_reply<H>(mut send: SendStream<H>, body: ReplyBody) -> Result<()>
where
    H: Hosted,
{
    let written = crate::rt::time::timeout(REPLY_WRITE_DEADLINE, async {
        match &body {
            ReplyBody::Framed(payload) => wire::write_payload(&mut send, payload).await,
            ReplyBody::Raw(bytes) => send
                .write_all(bytes)
                .await
                .map_err(|e| wire::vinc_error("write CAR", e)),
        }
    })
    .await;
    let Ok(written) = written else {
        send.reset(RESET_STALLED).await;
        return Err(Error::ResponseTimeout);
    };
    written.map_err(|error| Error::ResponseSend(error.to_string()))?;
    send.finish()
        .await
        .map_err(|e| Error::ResponseSend(format!("finish reply: {e}")))
}

/// A signed message whose signature verifies under its own public key, and
/// whose signer is the peer vinc authenticated.
pub(super) fn verify_signed_sender<M>(peer_id: &PeerId, msg: &M) -> Result<()>
where
    M: Message + serde::Serialize + Clone,
{
    if msg.sender_id() != peer_id.as_str() {
        return Err(Error::Transport(format!(
            "transport peer {peer_id} did not match signed sender {}",
            msg.sender_id()
        )));
    }
    let signature = msg.signature().ok_or(Error::MissingSignature)?;
    let pubkey: [u8; 32] = msg
        .pubkey()
        .try_into()
        .map_err(|_| Error::PublicKeyDecode("expected a 32-byte Ed25519 public key".into()))?;
    if hex::encode(pubkey) != peer_id.as_str() {
        return Err(Error::PubkeyPeerIdMismatch);
    }
    let key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey)
        .map_err(|error| Error::PublicKeyDecode(error.to_string()))?;
    let signature =
        ed25519_dalek::Signature::from_slice(signature).map_err(|_| Error::InvalidSignature)?;
    let mut unsigned = msg.clone();
    unsigned.set_signature(None);
    let bytes =
        defra_core::cbor::to_vec(&unsigned).map_err(|e| Error::CborSerialization(e.to_string()))?;
    key.verify_strict(&bytes, &signature)
        .map_err(|_| Error::InvalidSignature)
}

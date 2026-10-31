//! Go Shinzo's CAR channel on the shared CAR protocols.
//!
//! A Go `CARRequest` and `CARReply` are signed CBOR maps; Rust's own request
//! is a bare CID and its reply a bare CAR. An inbound request is told apart
//! by its first byte; an inbound reply is Go's only when it decodes as a
//! `CARReply` answering a request this node sent.

use std::time::Instant;

use cid::Cid;
use futures::AsyncWriteExt;
use libp2p::PeerId;

use super::{ensure_transport_sender, PendingResponseKey, PendingResponses, TwoStreamHandler};
use crate::error::{Error, Result};
use crate::message::car_exchange::{CarReply, CarRequest};
use crate::sync::shinzo_car::{accept_reply, CarPeerBackoff, Entry, Outcome};
use crate::two_stream::event::TwoStreamEvent;

impl PendingResponses {
    /// Remember a Go CAR request for `root`. A request older than the reply
    /// timeout is dropped first and counted against its peer.
    pub(crate) fn register_shinzo_car(&self, key: PendingResponseKey, root: Cid) {
        let now = Instant::now();
        let expired: Vec<_> = self
            .shinzo_cars
            .iter()
            .filter(|(_, (_, sent))| now.duration_since(*sent) > super::RESPONSE_TIMEOUT)
            .map(|(key, _)| key)
            .collect();
        for key in expired {
            if self.shinzo_cars.remove(&key).is_some() {
                self.car_backoff
                    .record(&key.peer_id.to_string(), Outcome::Fault, now);
            }
        }
        self.shinzo_cars.insert(key, (root, now));
    }

    /// Take the head a Go CAR request asked for, if it is still open.
    pub(crate) fn take_shinzo_car(&self, key: &PendingResponseKey) -> Option<Cid> {
        self.shinzo_cars.remove(key).map(|(root, _)| root)
    }

    pub(crate) fn car_backoff(&self) -> &CarPeerBackoff {
        &self.car_backoff
    }
}

/// Read a Go `CARRequest`, checking its signature and that it came from the
/// peer that signed it.
pub(super) fn decode_request(peer_id: &PeerId, data: &[u8]) -> Result<CarRequest> {
    let request: CarRequest = defra_core::cbor::from_slice(data)
        .map_err(|e| Error::CborDeserialization(format!("failed to decode CAR request: {e}")))?;
    crate::verify_message(&request)?;
    ensure_transport_sender(peer_id, &request)?;
    Ok(request)
}

/// Turn a Go `CARReply` into the CAR event the sync coordinator expects:
/// the head's CAR, or a CAR with no blocks so the coordinator fetches the
/// head another way. `None` when `data` is not a reply to a Go request this
/// node has open, so the caller reads it as a bare CAR.
pub(super) fn reply_event(
    pending: &PendingResponses,
    peer_id: PeerId,
    data: &[u8],
) -> Option<TwoStreamEvent> {
    let reply: CarReply = defra_core::cbor::from_slice(data).ok()?;
    let root = pending.take_shinzo_car(&PendingResponseKey::new(peer_id, &reply.message_id))?;
    let verified =
        crate::verify_message(&reply).and_then(|()| ensure_transport_sender(&peer_id, &reply));
    let car = match verified {
        Ok(()) => {
            pending
                .car_backoff()
                .record(&peer_id.to_string(), Outcome::Answered, Instant::now());
            match accept_reply(&[root], &reply) {
                Ok(entries) => match entries.into_iter().next() {
                    Some(Entry::Fetched(car)) => Some(car),
                    _ => None,
                },
                Err(error) => {
                    tracing::debug!(%peer_id, %root, %error, "Go CAR reply refused");
                    None
                }
            }
        }
        Err(error) => {
            tracing::warn!(%peer_id, %root, %error, "Go CAR reply failed verification");
            None
        }
    };
    let car_data = match car {
        Some(car) => car,
        None => crate::sync::car::encode_car_response(&[root], &[], &[]).ok()?,
    };
    Some(TwoStreamEvent::CarFetchResponse {
        peer_id,
        root_cid: root,
        car_data,
    })
}

impl TwoStreamHandler {
    /// Ask `peer_id` for the CAR rooted at `root` on Go Shinzo's channel.
    /// `request` must be signed; it is remembered until the reply comes or
    /// the reply timeout passes. A peer that keeps failing is not asked.
    pub async fn send_shinzo_car_request(
        &mut self,
        peer_id: PeerId,
        root: Cid,
        request: &CarRequest,
    ) -> Result<()> {
        let peer = peer_id.to_string();
        if self
            .pending
            .car_backoff()
            .passed_over(&peer, Instant::now())
        {
            return Err(Error::Transport(format!(
                "CAR requests to {peer_id} keep failing; not asking until its backoff ends"
            )));
        }
        let key = PendingResponseKey::new(peer_id, &request.message_id);
        self.pending.register_shinzo_car(key.clone(), root);
        let sent = self
            .write(peer_id, Self::car_request_protocol(), request)
            .await;
        if sent.is_err() {
            self.pending.take_shinzo_car(&key);
            self.pending
                .car_backoff()
                .record(&peer, Outcome::Fault, Instant::now());
        }
        sent
    }

    /// Answer a Go CAR request with a signed reply.
    pub async fn send_shinzo_car_reply(&mut self, peer_id: PeerId, reply: &CarReply) -> Result<()> {
        self.write(peer_id, Self::car_response_protocol(), reply)
            .await
    }

    async fn write<M>(
        &mut self,
        peer_id: PeerId,
        protocol: libp2p::StreamProtocol,
        message: &M,
    ) -> Result<()>
    where
        M: serde::Serialize,
    {
        let bytes = defra_core::cbor::to_vec(message)
            .map_err(|e| Error::Codec(format!("failed to encode Go CAR message: {e}")))?;
        let mut stream = self
            .control
            .open_stream(peer_id, protocol)
            .await
            .map_err(|e| Error::Transport(format!("failed to open Go CAR stream: {e}")))?;
        stream
            .write_all(&bytes)
            .await
            .map_err(|e| Error::Transport(format!("failed to write Go CAR message: {e}")))?;
        stream
            .close()
            .await
            .map_err(|e| Error::Transport(format!("failed to close Go CAR stream: {e}")))
    }
}

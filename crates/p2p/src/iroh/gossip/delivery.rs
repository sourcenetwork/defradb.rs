//! Turning what gossip delivers into transport events.

use iroh::EndpointId;
use iroh_gossip::proto::Event;
use tracing::{debug, warn};

use crate::message::PushLogBroadcast;
use crate::sync::DispatchSender;
use crate::transport::{MessageId, TransportEvent};

use super::super::endpoint::RawTopics;
use super::super::peer_map::endpoint_id_to_peer_id;
use crate::iroh::records::{Input as RecordsInput, RecordsHandle, RECORDS_TOPIC};

pub(crate) type EventSender = DispatchSender<TransportEvent<iroh::endpoint::SendStream>>;

/// The event channel is gone: the node is shutting down.
pub(super) struct Closed;

pub(super) struct Delivery {
    pub(super) event_tx: EventSender,
    pub(super) raw_topics: RawTopics,
    pub(super) records: RecordsHandle,
}

impl Delivery {
    /// Our first connection to `peer`: the records task looks up its record.
    pub(super) fn connected(&self, peer: EndpointId) {
        self.records.push(RecordsInput::Connected(peer));
    }

    /// `peer` has no connection left.
    pub(super) fn disconnected(&self, peer: EndpointId) {
        self.records.push(RecordsInput::Disconnected(peer));
    }

    pub(super) async fn neighbor(
        &self,
        topic: &str,
        peer: EndpointId,
        up: bool,
    ) -> Result<(), Closed> {
        let peer_id = endpoint_id_to_peer_id(&peer);
        let topic = topic.to_string();
        let event = if up {
            TransportEvent::PeerSubscribed { peer_id, topic }
        } else {
            TransportEvent::PeerUnsubscribed { peer_id, topic }
        };
        self.event_tx.send(event).await.map_err(|_| Closed)
    }

    pub(super) async fn received(
        &self,
        topic: &str,
        event: Event<EndpointId>,
    ) -> Result<(), Closed> {
        let Event::Received(msg) = event else {
            return Ok(());
        };
        if topic == RECORDS_TOPIC {
            self.records.push(RecordsInput::Announced(msg.content));
            return Ok(());
        }
        let sender_peer_id = endpoint_id_to_peer_id(&msg.delivered_from);
        let message_id = MessageId::new(uuid::Uuid::new_v4().to_string());
        if self.raw_topics.contains_key(topic) {
            let event = TransportEvent::GossipRawMessage {
                propagation_source: sender_peer_id,
                message_id,
                topic: topic.to_string(),
                data: msg.content.to_vec(),
            };
            return self.event_tx.send(event).await.map_err(|_| Closed);
        }
        let is_direct = msg.scope.is_direct();
        let (mut broadcast, encoding) = match PushLogBroadcast::decode_gossip_payload(&msg.content)
        {
            Ok(decoded) => decoded,
            Err(error) => {
                record_decode_failure(&sender_peer_id, topic, &msg.content, error);
                return Ok(());
            }
        };
        let origin =
            match authenticate_pushlog_origin(&mut broadcast, &msg.delivered_from, is_direct) {
                Ok(origin) => origin,
                Err(error) => {
                    warn!(
                        peer_id = %sender_peer_id,
                        claimed_source_peer_id = ?broadcast.source_peer_id,
                        topic,
                        is_direct,
                        error = %error,
                        "Dropping Iroh head hint with unauthenticated origin"
                    );
                    return Ok(());
                }
            };
        debug!(
            peer_id = %sender_peer_id,
            origin_peer_id = %origin,
            is_direct,
            topic,
            "Authenticated Iroh head-hint origin and ingress hop"
        );
        if encoding != crate::message::PushLogGossipPayloadEncoding::PostcardBroadcast {
            debug!(
                peer_id = %sender_peer_id,
                topic,
                message_size = msg.content.len(),
                ?encoding,
                "Decoded Iroh gossip message via compatibility fallback"
            );
        }
        let event = TransportEvent::GossipMessage {
            propagation_source: sender_peer_id,
            message_id,
            topic: topic.to_string(),
            message: broadcast,
        };
        self.event_tx.send(event).await.map_err(|_| Closed)
    }
}

fn record_decode_failure(
    sender_peer_id: &crate::transport::PeerId,
    topic: &str,
    content: &[u8],
    error: String,
) {
    let payload_info = PushLogBroadcast::inspect_gossip_payload(content);
    let sample = crate::sync::GossipDecodeFailureSample {
        transport: crate::sync::GossipTransport::Iroh,
        peer_id: sender_peer_id.to_string(),
        topic: topic.to_string(),
        message_size: content.len(),
        error: error.clone(),
        payload_fingerprint: payload_info.payload_fingerprint,
        payload_shape_hint: payload_info.payload_shape_hint,
        occurrences: 0,
    };
    // Warn on the 1st, 2nd, 4th, 8th... failure and log the rest at debug.
    // The count is surfaced through SyncDiagnostics.
    let count = crate::sync::record_gossip_decode_failure_sample(sample.clone());
    if count.is_power_of_two() {
        warn!(
            peer_id = %sender_peer_id,
            topic,
            message_size = content.len(),
            total_failures = count,
            error = %error,
            payload_fingerprint = %sample.payload_fingerprint,
            payload_shape = %sample.payload_shape_hint,
            "Failed to decode Iroh gossip message as PushLogBroadcast or PushLogRequest"
        );
    } else {
        debug!(
            peer_id = %sender_peer_id,
            topic,
            message_size = content.len(),
            total_failures = count,
            error = %error,
            "Failed to decode Iroh gossip message"
        );
    }
}

/// Authenticate the endpoint that originated a PushLog gossip envelope.
///
/// Iroh's `delivered_from` authenticates only the last hop. New publishers
/// therefore sign the canonical envelope with their endpoint key. The signed
/// origin proves who emitted the hint. The connected hop is retained only as
/// ingress metadata: a gossip relay may not own the linked DAG. During a
/// rolling upgrade, an unsigned envelope is accepted only when Iroh proves it
/// arrived directly from its publisher; any payload `SourcePeerID` is ignored.
fn authenticate_pushlog_origin(
    broadcast: &mut PushLogBroadcast,
    delivered_from: &iroh::EndpointId,
    is_direct: bool,
) -> crate::error::Result<iroh::EndpointId> {
    match (
        broadcast.source_peer_id.as_deref(),
        broadcast.origin_signature.as_deref(),
    ) {
        (Some(claimed_source), Some(signature_bytes)) => {
            let origin: iroh::EndpointId =
                claimed_source
                    .parse()
                    .map_err(|error: iroh::KeyParsingError| {
                        crate::error::Error::InvalidPeerId(error.to_string())
                    })?;
            let signature = iroh::Signature::try_from(signature_bytes)
                .map_err(|_| crate::error::Error::InvalidSignature)?;
            let signing_bytes = broadcast.origin_signing_bytes()?;
            origin
                .verify(&signing_bytes, &signature)
                .map_err(|_| crate::error::Error::InvalidSignature)?;
            broadcast.authenticate_origin_peer(origin.to_string());
            broadcast.authenticate_source_peer(delivered_from.to_string());
            Ok(origin)
        }
        (_, Some(_)) => Err(crate::error::Error::InvalidSignature),
        (_, None) if is_direct => {
            let origin = *delivered_from;
            // Never retain an unsigned payload claim, even on the compatible
            // direct path. Iroh's authenticated delivery metadata is the
            // authority in this case.
            broadcast.source_peer_id = Some(origin.to_string());
            broadcast.authenticate_origin_peer(origin.to_string());
            broadcast.authenticate_source_peer(origin.to_string());
            Ok(origin)
        }
        (_, None) => Err(crate::error::Error::MissingSignature),
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    fn broadcast() -> PushLogBroadcast {
        PushLogBroadcast::new(
            "doc".to_string(),
            Bytes::from_static(&[1, 2, 3]),
            "collection".to_string(),
            "creator".to_string(),
            Bytes::from_static(&[4, 5, 6]),
        )
    }

    #[test]
    fn relayed_origin_is_verified_and_hop_is_only_ingress_metadata() {
        let origin = iroh::SecretKey::generate();
        let relay = iroh::SecretKey::generate().public();
        let mut message = broadcast();
        message.source_peer_id = Some(origin.public().to_string());
        let bytes = message.origin_signing_bytes().unwrap();
        message.origin_signature = Some(origin.sign(&bytes).to_bytes().to_vec());

        let authenticated = authenticate_pushlog_origin(&mut message, &relay, false).unwrap();
        assert_eq!(authenticated, origin.public());
        assert_eq!(
            message.authenticated_origin_peer_id(),
            Some(origin.public().to_string().as_str())
        );
        assert_eq!(
            message.authenticated_source_peer_id(),
            Some(relay.to_string().as_str())
        );
    }

    #[test]
    fn forged_relayed_origin_is_rejected() {
        let claimed = iroh::SecretKey::generate();
        let forger = iroh::SecretKey::generate();
        let relay = iroh::SecretKey::generate().public();
        let mut message = broadcast();
        message.source_peer_id = Some(claimed.public().to_string());
        let bytes = message.origin_signing_bytes().unwrap();
        message.origin_signature = Some(forger.sign(&bytes).to_bytes().to_vec());

        assert!(matches!(
            authenticate_pushlog_origin(&mut message, &relay, false),
            Err(crate::error::Error::InvalidSignature)
        ));
        assert!(message.authenticated_source_peer_id().is_none());
        assert!(message.authenticated_origin_peer_id().is_none());
    }

    #[test]
    fn rolling_unsigned_compatibility_is_direct_only_and_ignores_claim() {
        let direct = iroh::SecretKey::generate().public();
        let relay = iroh::SecretKey::generate().public();
        let mut direct_message = broadcast();
        direct_message.source_peer_id = Some("forged".to_string());
        assert_eq!(
            authenticate_pushlog_origin(&mut direct_message, &direct, true).unwrap(),
            direct
        );
        assert_eq!(
            direct_message.authenticated_source_peer_id(),
            Some(direct.to_string().as_str())
        );
        assert_eq!(
            direct_message.authenticated_origin_peer_id(),
            Some(direct.to_string().as_str())
        );

        let mut relayed_message = broadcast();
        relayed_message.source_peer_id = Some("forged".to_string());
        assert!(matches!(
            authenticate_pushlog_origin(&mut relayed_message, &relay, false),
            Err(crate::error::Error::MissingSignature)
        ));
    }
}

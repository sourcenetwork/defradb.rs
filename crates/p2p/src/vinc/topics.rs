//! Topic pubsub over vinc's hub broadcast: join, leave, publish, the peers of
//! a topic, and what the endpoint's topic events become.
//!
//! vinc carries topics through hubs (the relays the endpoint names), and the
//! publisher signs every message, which vinc verifies before it hands it on.
//! So the origin of a message is authenticated whichever hub carried it, and
//! it is the peer that holds the blocks a head hint names.
//!
//! What libp2p and iroh do that a hub does not do by itself, and how this
//! file matches it:
//!
//! - **JOINED and LEFT.** vinc reports a topic's members this endpoint has a
//!   session with (`NeighborUp`, `NeighborDown`), which is what a gossip mesh
//!   reports: a connected peer that subscribed to a topic this node joined.
//! - **Reach.** A hub delivers to every member of the topic, connected to its
//!   publisher or not; a gossip mesh reaches only peers it is connected to,
//!   directly or through other peers. A node with no peer session is on no
//!   mesh, so it takes nothing a hub hands it. Modeled in
//!   `proofs/tla/VincTopicDelivery.tla`.
//! - **Raw topics.** A raw topic is marked raw before it is joined, so none
//!   of its messages is ever decoded as a PushLog.

use std::rc::Rc;
use std::time::Duration;

use ed25519_dalek::Verifier as _;
use tracing::{debug, warn};
use vinc::{Event, Hosted, NodeId};

use crate::error::{Error, Result};
use crate::message::PushLogBroadcast;
use crate::transport::{MessageId, PeerId, TransportEvent};

use super::addr::node_to_peer_id;
use super::command::Reply;
use super::endpoint::Node;
use super::wire::vinc_error;

/// How long a waiting publish holds for a member of its topic, as on iroh.
const NEIGHBOR_WAIT: Duration = Duration::from_secs(10);

impl<H: Hosted + 'static> Node<H> {
    pub(super) async fn subscribe(&self, topic: String, raw: bool, reply: Reply<bool>) {
        if raw {
            self.raw_topics.borrow_mut().insert(topic.clone());
        }
        // The engine's before the join, so no event of it is taken as foreign.
        self.subscribed.borrow_mut().insert(topic.clone());
        let joined = self
            .ep
            .join(topic.as_bytes(), &[])
            .await
            .map_err(|error| vinc_error("topic join", error));
        if joined.is_err() {
            self.subscribed.borrow_mut().remove(&topic);
        }
        let _ = reply.send(joined);
    }

    pub(super) async fn unsubscribe(&self, topic: &str, reply: Reply<bool>) {
        self.subscribed.borrow_mut().remove(topic);
        let _ = reply.send(Ok(self.ep.leave(topic.as_bytes()).await));
    }

    pub(super) fn register_raw_topic(&self, topic: String) {
        self.raw_topics.borrow_mut().insert(topic);
    }

    /// Publish now, or, with `wait`, answer at once and publish from a task
    /// that first waits for a member of the topic, as iroh does: neither the
    /// caller nor the command loop waits on one.
    pub(super) async fn publish(
        self: &Rc<Self>,
        topic: String,
        payload: Vec<u8>,
        wait: bool,
        reply: Reply<MessageId>,
    ) {
        if !wait {
            let _ = reply.send(self.publish_now(&topic, &payload, None).await);
            return;
        }
        let publisher = Rc::clone(self);
        let spawned = self.spawn(async move {
            // The hub tells a topic's members to its members: a node that has
            // not joined learns none, so it joins for the publish, as iroh does.
            let ephemeral = publisher
                .ep
                .join(topic.as_bytes(), &[])
                .await
                .unwrap_or(false);
            let waited = publisher
                .publish_now(&topic, &payload, Some(NEIGHBOR_WAIT))
                .await;
            // No member showed up in time: hand it to the hub anyway.
            let result = match waited {
                Err(Error::ConnectionTimeout(_)) => {
                    publisher.publish_now(&topic, &payload, None).await
                }
                result => result,
            };
            if let Err(error) = result {
                debug!(topic, %error, "vinc waiting publish failed");
            }
            // Leave only a join this publish made, and not if the engine
            // subscribed to the topic meanwhile.
            if ephemeral && !publisher.subscribed.borrow().contains(&topic) {
                publisher.ep.leave(topic.as_bytes()).await;
            }
        });
        let _ = reply.send(match spawned {
            Some(_) => Ok(MessageId::new(uuid::Uuid::new_v4().to_string())),
            None => Err(Error::Transport("the vinc endpoint is busy".to_owned())),
        });
    }

    async fn publish_now(
        &self,
        topic: &str,
        payload: &[u8],
        wait: Option<Duration>,
    ) -> Result<MessageId> {
        match self.ep.publish(topic.as_bytes(), payload, wait).await {
            Ok(id) => Ok(MessageId::new(hex::encode(id))),
            Err(error) if error.kind() == vinc::ErrorKind::TimedOut => {
                Err(Error::ConnectionTimeout(topic.to_owned()))
            }
            Err(error) => Err(vinc_error("topic publish", error)),
        }
    }

    pub(super) fn topic_peers(&self, topic: &str) -> Vec<PeerId> {
        self.ep
            .neighbors(topic.as_bytes())
            .iter()
            .map(node_to_peer_id)
            .collect()
    }

    /// Turn the endpoint's topic events into transport events until it stops.
    /// This is the endpoint's one reader of them.
    pub(super) async fn topic_events(self: Rc<Self>) {
        while let Some(event) = self.ep.event().await {
            // Only a topic the engine subscribed to is the engine's: not one
            // joined only to publish, nor one left while its events queued.
            let foreign = match &event {
                Event::NeighborUp { topic, .. }
                | Event::NeighborDown { topic, .. }
                | Event::Message { topic, .. } => !self
                    .subscribed
                    .borrow()
                    .contains(topic_name(topic).as_str()),
                Event::Lost { .. } => false,
            };
            if foreign {
                continue;
            }
            let event = match event {
                Event::NeighborUp { topic, peer } => TransportEvent::PeerSubscribed {
                    peer_id: node_to_peer_id(&peer),
                    topic: topic_name(&topic),
                },
                Event::NeighborDown { topic, peer } => TransportEvent::PeerUnsubscribed {
                    peer_id: node_to_peer_id(&peer),
                    topic: topic_name(&topic),
                },
                Event::Message {
                    topic,
                    origin,
                    payload,
                    ..
                } => match self.delivery(topic_name(&topic), origin, payload) {
                    Some(event) => event,
                    None => continue,
                },
                Event::Lost { count } => {
                    warn!(count, "vinc topic events were dropped: the queue was full");
                    continue;
                }
            };
            if self.events.send(event).await.is_err() {
                debug!("Event channel closed, vinc topic events stop");
                return;
            }
        }
    }

    /// The transport event a message becomes, or `None` when it is not
    /// delivered.
    fn delivery(&self, topic: String, origin: NodeId, payload: Vec<u8>) -> Option<EventOut> {
        // ponytail: "on a mesh" is "holds a peer session"; two meshes that
        // share a hub still hear each other. Exact reach needs the hub to know
        // who is connected to whom (vinc facade gap).
        if self.peers.borrow().is_empty() {
            debug!(topic, "vinc topic message dropped: this node has no peer");
            return None;
        }
        let origin_id = node_to_peer_id(&origin);
        let message_id = MessageId::new(uuid::Uuid::new_v4().to_string());
        if self.raw_topics.borrow().contains(&topic) {
            return Some(TransportEvent::GossipRawMessage {
                propagation_source: origin_id,
                message_id,
                topic,
                data: payload,
            });
        }
        let (mut broadcast, _) = PushLogBroadcast::decode_gossip_payload(&payload)
            .inspect_err(
                |error| debug!(peer_id = %origin_id, topic, %error, "undecodable vinc head hint"),
            )
            .ok()?;
        authenticate_origin(&mut broadcast, &origin_id)
            .inspect_err(|error| {
                warn!(peer_id = %origin_id, topic, %error, "dropping vinc head hint with an unauthenticated origin");
            })
            .ok()?;
        Some(TransportEvent::GossipMessage {
            propagation_source: origin_id,
            message_id,
            topic,
            message: broadcast,
        })
    }
}

type EventOut = super::command::EventOut;

fn topic_name(topic: &[u8]) -> String {
    String::from_utf8_lossy(topic).into_owned()
}

/// vinc verified `publisher`'s signature over the payload, so it is the
/// provider. A payload that names another origin must carry that origin's
/// signature, as on iroh.
fn authenticate_origin(broadcast: &mut PushLogBroadcast, publisher: &PeerId) -> Result<()> {
    let origin = match (
        broadcast.source_peer_id.clone(),
        broadcast.origin_signature.as_deref(),
    ) {
        (Some(claimed), Some(signature)) if claimed != publisher.as_str() => {
            verify_origin(&claimed, signature, &broadcast.origin_signing_bytes()?)?;
            claimed
        }
        (None, Some(_)) => return Err(Error::InvalidSignature),
        _ => publisher.to_string(),
    };
    broadcast.source_peer_id = Some(origin.clone());
    broadcast.authenticate_origin_peer(origin);
    broadcast.authenticate_source_peer(publisher.to_string());
    Ok(())
}

fn verify_origin(claimed: &str, signature: &[u8], signed: &[u8]) -> Result<()> {
    let key: [u8; 32] = hex::decode(claimed)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| Error::InvalidPeerId(format!("not a vinc peer id: {claimed}")))?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&key).map_err(|_| Error::InvalidSignature)?;
    let signature =
        ed25519_dalek::Signature::from_slice(signature).map_err(|_| Error::InvalidSignature)?;
    key.verify(signed, &signature)
        .map_err(|_| Error::InvalidSignature)
}

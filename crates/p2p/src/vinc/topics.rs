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
//!   session with (`NeighborUp`, `NeighborDown`), but only on a topic it
//!   joined. gossipsub tells a connected peer's subscriptions on every topic,
//!   so a node that publishes on a document topic it never subscribed to
//!   still hears who joins and leaves it. Here every publish holds its topic
//!   joined for [`FANOUT_TTL`] after it, gossipsub's fanout time: the engine
//!   hears JOINED and LEFT for it, but none of its messages.
//! - **Reach.** A hub delivers to every member of the topic, connected to its
//!   publisher or not; a gossip mesh reaches only peers it is connected to,
//!   directly or through other peers. A node with no peer session is on no
//!   mesh, so it takes nothing a hub hands it. Modeled in
//!   `proofs/tla/VincTopicDelivery.tla`.
//! - **Raw topics.** A raw topic is marked raw before it is joined, so none
//!   of its messages is ever decoded as a PushLog.

use std::rc::Rc;
use std::time::{Duration, Instant};

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

/// How long a publish keeps its topic joined, gossipsub's default
/// `fanout_ttl`.
pub(super) const FANOUT_TTL: Duration = Duration::from_secs(60);

/// The most topics held for publishes at once: half of vinc's default topic
/// limit (`Limits::topics`, 128), the rest left to the engine's subscriptions.
const MAX_FANOUT: usize = 64;

impl<H: Hosted + 'static> Node<H> {
    pub(super) async fn subscribe(&self, topic: String, raw: bool, reply: Reply<bool>) {
        if raw {
            self.raw_topics.borrow_mut().insert(topic.clone());
        }
        // The engine's before the join, so no event of it is taken as foreign.
        let new = self.subscribed.borrow_mut().insert(topic.clone());
        let joined = self
            .ep
            .join(topic.as_bytes(), &[])
            .await
            .map(|_| new)
            .map_err(|error| vinc_error("topic join", error));
        if joined.is_err() && new {
            self.subscribed.borrow_mut().remove(&topic);
        }
        let _ = reply.send(joined);
    }

    /// Keep `topic` joined for [`FANOUT_TTL`] from now: a new hold joins it.
    async fn hold_fanout(&self, topic: &str) {
        let until = Instant::now() + FANOUT_TTL;
        let held = {
            let mut fanout = self.fanout.borrow_mut();
            // ponytail: past MAX_FANOUT a publish is sent unheld (no JOINED or
            // LEFT for its topic) so engine subscriptions keep their room in
            // vinc's topic limit; evict the oldest hold if bursts need it.
            if fanout.len() >= MAX_FANOUT && !fanout.contains_key(topic) {
                return;
            }
            fanout.insert(topic.to_owned(), until)
        };
        if held.is_none() && !self.subscribed.borrow().contains(topic) {
            if let Err(error) = self.ep.join(topic.as_bytes(), &[]).await {
                debug!(topic, %error, "vinc fanout join failed");
                self.fanout.borrow_mut().remove(topic);
            }
        }
    }

    /// Leave the fanout topics whose hold ended by `now`, unless the engine
    /// subscribed to them.
    pub(super) async fn expire_fanout(&self, now: Instant) {
        let ended: Vec<String> = {
            let mut fanout = self.fanout.borrow_mut();
            let ended = fanout
                .iter()
                .filter(|(_, until)| **until <= now)
                .map(|(topic, _)| topic.clone())
                .collect::<Vec<_>>();
            for topic in &ended {
                fanout.remove(topic);
            }
            ended
        };
        for topic in ended {
            if !self.subscribed.borrow().contains(&topic) {
                self.ep.leave(topic.as_bytes()).await;
            }
        }
    }

    pub(super) async fn unsubscribe(&self, topic: &str, reply: Reply<bool>) {
        let was = self.subscribed.borrow_mut().remove(topic);
        // A fanout hold keeps the topic until it ends.
        if !self.fanout.borrow().contains_key(topic) {
            self.ep.leave(topic.as_bytes()).await;
        }
        let _ = reply.send(Ok(was));
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
        // The hub tells a topic's members only to its members, so a waiting
        // publish must be on the topic to see one arrive.
        self.hold_fanout(&topic).await;
        if !wait {
            let _ = reply.send(self.publish_now(&topic, &payload, None).await);
            return;
        }
        let publisher = Rc::clone(self);
        let spawned = self.spawn(async move {
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
            // Messages only of a topic the engine subscribed to: not one held
            // only for its publishes, nor one left while its events queued.
            // Who joins and leaves, of either.
            let foreign = match &event {
                Event::NeighborUp { topic, .. } | Event::NeighborDown { topic, .. } => {
                    let topic = topic_name(topic);
                    !self.subscribed.borrow().contains(&topic)
                        && !self.fanout.borrow().contains_key(&topic)
                }
                Event::Message { topic, .. } => !self
                    .subscribed
                    .borrow()
                    .contains(topic_name(topic).as_str()),
                Event::Lost { .. }
                | Event::AdjacencyShort { .. }
                | Event::ReachRefused { .. }
                | Event::ReachClosed
                | Event::ReachOpened => false,
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
                    hop,
                    payload,
                    ..
                } => match self.delivery(topic_name(&topic), origin, &hop, payload) {
                    Some(event) => event,
                    None => continue,
                },
                Event::Lost { count } => {
                    warn!(count, "vinc topic events were dropped: the queue was full");
                    continue;
                }
                Event::AdjacencyShort { sessions, named } => {
                    warn!(sessions, ?named, "vinc hubs were told only some of this node's peers");
                    continue;
                }
                Event::ReachRefused { node } => {
                    warn!(peer_id = %node_to_peer_id(&node), "vinc hub adjacency table full: a peer's record was refused");
                    continue;
                }
                Event::ReachClosed => {
                    warn!("vinc hub adjacency table closed");
                    continue;
                }
                Event::ReachOpened => {
                    debug!("vinc hub adjacency table opened again");
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
    fn delivery(
        &self,
        topic: String,
        origin: NodeId,
        hop: &NodeId,
        payload: Vec<u8>,
    ) -> Option<EventOut> {
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
                propagation_source: node_to_peer_id(hop),
                source: origin_id,
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

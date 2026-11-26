//! Topic pubsub over pinc's hub broadcast: join, leave, publish, the peers of
//! a topic, and what the endpoint's topic events become.
//!
//! pinc carries topics through hubs (the relays the endpoint names), and the
//! publisher signs every message, which pinc verifies before it hands it on.
//! So the origin of a message is authenticated whichever hub carried it, and
//! it is the peer that holds the blocks a head hint names.
//!
//! What the sync engine expects of gossip (gossipsub's behaviour) that a hub
//! does not do by itself, and how this file matches it:
//!
//! - **JOINED and LEFT.** pinc reports a topic's members this endpoint has a
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
//!   `proofs/tla/PincTopicDelivery.tla`.
//! - **Subscribed means reachable.** A gossipsub subscription is in the
//!   peers' view once the call returns; a pinc join returns before any hub
//!   knows of it, so a message published at once could miss the new member.
//!   A subscribe answers once a hub confirmed the topic's join
//!   (`Event::Joined`), whichever join made it, and fails after
//!   [`JOIN_WAIT`] with none: a subscription no hub holds would look
//!   installed and get nothing. A failed subscribe takes the topic back, so
//!   the coordinator's retry joins afresh and waits again. Modeled in
//!   `proofs/tla/PincJoinConfirm.tla`.
//! - **Raw topics.** A raw topic is marked raw before it is joined, so none
//!   of its messages is ever decoded as a PushLog.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use ed25519_dalek::Verifier as _;
use pinc::{Event, Hosted, NodeId};
use tracing::{debug, warn};

use p2p::error::{Error, Result};
use p2p::message::PushLogBroadcast;
use p2p::rt::oneshot;
use p2p::transport::{MessageId, PeerId, TransportEvent};

use super::addr::node_to_peer_id;
use super::command::Reply;
use super::endpoint::Node;
use super::hub::MAX_TOPIC_MESSAGE;
use super::wire::pinc_error;

/// How long a waiting publish holds for a member of its topic.
const NEIGHBOR_WAIT: Duration = Duration::from_secs(10);

/// How long a subscribe waits for a hub to confirm its join.
/// Under the coordinator's subscription deadline (15 s), so a join no hub
/// confirms fails as itself, not as the coordinator's timeout.
const JOIN_WAIT: Duration = Duration::from_secs(12);

/// How long a publish keeps its topic joined, gossipsub's default
/// `fanout_ttl`.
pub(super) const FANOUT_TTL: Duration = Duration::from_secs(60);

/// The most topics held for publishes at once: half of pinc's default topic
/// limit (`Limits::topics`, 128), the rest left to the engine's subscriptions.
const MAX_FANOUT: usize = 64;

/// The publishes that failed where no caller hears of it: a waiting publish
/// answers before it is sent. Each is counted and logged at warn.
#[derive(Debug, Default)]
pub(super) struct PublishCounters {
    oversized: AtomicU64,
    failed_after_reply: AtomicU64,
}

impl PublishCounters {
    pub(super) fn snapshot(&self) -> PublishStats {
        PublishStats {
            oversized: self.oversized.load(Ordering::Relaxed),
            failed_after_reply: self.failed_after_reply.load(Ordering::Relaxed),
        }
    }
}

/// How many of a node's publishes never reached its hubs, by why.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PublishStats {
    /// Refused, to their caller, as larger than one topic message
    /// (`MAX_TOPIC_MESSAGE`).
    pub oversized: u64,
    /// Waiting publishes that failed after their caller was answered.
    pub failed_after_reply: u64,
}

impl<H: Hosted + 'static> Node<H> {
    pub(super) async fn subscribe(self: &Rc<Self>, topic: String, raw: bool, reply: Reply<bool>) {
        if raw {
            self.raw_topics.borrow_mut().insert(topic.clone());
        }
        // The engine's before the join, so no event of it is taken as foreign.
        let new = self.subscribed.borrow_mut().insert(topic.clone());
        let joined = self
            .ep
            .join(topic.as_bytes(), &[])
            .await
            .map_err(|error| pinc_error("topic join", error));
        if joined.is_err() && new {
            self.subscribed.borrow_mut().remove(&topic);
        }
        // A topic a publish holds joined may not be confirmed yet, and one a
        // failed subscribe left joined never answers again: whether the join
        // is new to pinc says nothing, only a confirmation does.
        match joined {
            Ok(_) if self.has_hubs && !self.confirmed.borrow().contains(&topic) => {
                self.answer_when_confirmed(topic, new, reply).await;
            }
            joined => {
                let _ = reply.send(joined.map(|_| new));
            }
        }
    }

    /// Answer a subscription once a hub confirms its join, or fail it after
    /// [`JOIN_WAIT`]: the command loop does not wait. With no room for the
    /// waiting task, the join is taken back and the subscribe fails now.
    async fn answer_when_confirmed(self: &Rc<Self>, topic: String, new: bool, reply: Reply<bool>) {
        let (confirmed, wait) = oneshot::channel();
        self.joining
            .borrow_mut()
            .entry(topic.clone())
            .or_default()
            .push(confirmed);
        // Shared with the task, so a refused spawn still holds the reply.
        let reply = Rc::new(Cell::new(Some(reply)));
        let node = Rc::clone(self);
        let waiting_topic = topic.clone();
        let task_reply = Rc::clone(&reply);
        let spawned = self.spawn(async move {
            let topic = waiting_topic;
            let woken = matches!(p2p::rt::time::timeout(JOIN_WAIT, wait).await, Ok(Ok(())));
            // A confirmation that landed as the wait ran out still counts.
            let answer = if woken || node.confirmed.borrow().contains(&topic) {
                Ok(new)
            } else {
                node.abandon_join(&topic).await;
                Err(Error::Transport(format!(
                    "no pinc hub confirmed the join of {topic} within {JOIN_WAIT:?}"
                )))
            };
            if let Some(reply) = task_reply.take() {
                let _ = reply.send(answer);
            }
        });
        if spawned.is_none() {
            // The refused task dropped its receiver, so abandoning the join
            // also drops this subscribe's closed sender from `joining`.
            self.abandon_join(&topic).await;
            if let Some(reply) = reply.take() {
                let _ = reply.send(Err(Error::Transport(format!(
                    "the pinc endpoint is busy: the join of {topic} was taken back"
                ))));
            }
        }
    }

    /// A hub confirmed the join of `topic`: if the topic is still joined,
    /// remember it and answer its waiting subscribes. A subscribe waiting on a
    /// topic since left fails when its wait runs out.
    fn joined(&self, topic: &str) {
        // ponytail: a confirmation of an earlier join, still queued when the
        // topic is left and joined again, counts for the new join; pinc #483
        // is the hub confirming only the subscription that asked.
        if !self.subscribed.borrow().contains(topic) && !self.fanout.borrow().contains_key(topic) {
            return;
        }
        self.confirmed.borrow_mut().insert(topic.to_owned());
        for confirmed in self.joining.borrow_mut().remove(topic).unwrap_or_default() {
            let _ = confirmed.send(());
        }
    }

    /// No hub confirmed `topic` in time: take the subscription back, and
    /// leave unless a publish holds the topic, so the retry joins afresh. A
    /// confirmation pinc dropped (`Event::Lost`) never comes again for a
    /// join that stays.
    async fn abandon_join(&self, topic: &str) {
        {
            let mut joining = self.joining.borrow_mut();
            if let Some(waiting) = joining.get_mut(topic) {
                waiting.retain(|confirmed| !confirmed.is_closed());
                if waiting.is_empty() {
                    joining.remove(topic);
                }
            }
        }
        self.subscribed.borrow_mut().remove(topic);
        if !self.fanout.borrow().contains_key(topic) {
            self.leave(topic).await;
        }
    }

    /// Leave `topic`: pinc forgets its confirmation, and so does this node.
    async fn leave(&self, topic: &str) {
        self.confirmed.borrow_mut().remove(topic);
        self.ep.leave(topic.as_bytes()).await;
    }

    /// Keep `topic` joined for [`FANOUT_TTL`] from now: a new hold joins it.
    async fn hold_fanout(&self, topic: &str) {
        let until = Instant::now() + FANOUT_TTL;
        let held = {
            let mut fanout = self.fanout.borrow_mut();
            // ponytail: past MAX_FANOUT a publish is sent unheld (no JOINED or
            // LEFT for its topic) so engine subscriptions keep their room in
            // pinc's topic limit; evict the oldest hold if bursts need it.
            if fanout.len() >= MAX_FANOUT && !fanout.contains_key(topic) {
                return;
            }
            fanout.insert(topic.to_owned(), until)
        };
        if held.is_none() && !self.subscribed.borrow().contains(topic) {
            if let Err(error) = self.ep.join(topic.as_bytes(), &[]).await {
                debug!(topic, %error, "pinc fanout join failed");
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
                self.leave(&topic).await;
            }
        }
    }

    pub(super) async fn unsubscribe(&self, topic: &str, reply: Reply<bool>) {
        let was = self.subscribed.borrow_mut().remove(topic);
        // Leave now, fanout hold or not: gossipsub tells its peers at once, so
        // a node that published to the topic still shows as LEFT. The hold
        // ends with it; the next publish holds the topic again.
        if was {
            self.fanout.borrow_mut().remove(topic);
        }
        if was || !self.fanout.borrow().contains_key(topic) {
            self.leave(topic).await;
        }
        let _ = reply.send(Ok(was));
    }

    pub(super) fn register_raw_topic(&self, topic: String) {
        self.raw_topics.borrow_mut().insert(topic);
    }

    /// Publish now, or, with `wait`, answer at once and publish from a task
    /// that first waits for a member of the topic: neither the
    /// caller nor the command loop waits on one.
    pub(super) async fn publish(
        self: &Rc<Self>,
        topic: String,
        payload: Vec<u8>,
        wait: bool,
        reply: Reply<MessageId>,
    ) {
        // A payload no hub takes fails its caller here, before a waiting
        // publish answers and the failure has nobody to reach.
        if payload.len() > MAX_TOPIC_MESSAGE {
            self.publishes.oversized.fetch_add(1, Ordering::Relaxed);
            warn!(
                topic,
                size = payload.len(),
                limit = MAX_TOPIC_MESSAGE,
                "pinc publish refused: larger than one topic message"
            );
            let _ = reply.send(Err(Error::GossipTooLarge {
                size: payload.len(),
                limit: MAX_TOPIC_MESSAGE,
            }));
            return;
        }
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
                publisher
                    .publishes
                    .failed_after_reply
                    .fetch_add(1, Ordering::Relaxed);
                warn!(topic, %error, "pinc waiting publish failed after its caller was answered");
            }
        });
        let _ = reply.send(match spawned {
            Some(_) => Ok(self.next_message_id()),
            None => Err(Error::Transport("the pinc endpoint is busy".to_owned())),
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
            Err(error) if error.kind() == pinc::ErrorKind::TimedOut => {
                Err(Error::ConnectionTimeout(topic.to_owned()))
            }
            Err(error) => Err(pinc_error("topic publish", error)),
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
                | Event::Missed { .. }
                | Event::Joined { .. }
                | Event::HubFull { .. }
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
                    warn!(count, "pinc topic events were dropped: the queue was full");
                    continue;
                }
                Event::Missed { topic, hub } => {
                    let topic = topic_name(&topic);
                    let hub = node_to_peer_id(&hub);
                    warn!(%topic, %hub, "a pinc hub says messages of a topic may be missing");
                    TransportEvent::TopicMissed { topic, hub }
                }
                Event::Joined { topic, .. } => {
                    self.joined(&topic_name(&topic));
                    continue;
                }
                Event::HubFull {
                    topic,
                    hub,
                    message: Some(_),
                } => {
                    warn!(topic = %topic_name(&topic), hub = %node_to_peer_id(&hub), "no pinc hub could hold a publish; it is not sent again");
                    continue;
                }
                Event::HubFull {
                    topic,
                    hub,
                    message: None,
                } => {
                    warn!(topic = %topic_name(&topic), hub = %node_to_peer_id(&hub), "a pinc hub cannot hold this node's subscription yet; the join is retried");
                    continue;
                }
                Event::AdjacencyShort { sessions, named } => {
                    warn!(
                        sessions,
                        ?named,
                        "pinc hubs were told only some of this node's peers"
                    );
                    continue;
                }
                Event::ReachRefused { node } => {
                    warn!(peer_id = %node_to_peer_id(&node), "pinc hub adjacency table full: a peer's record was refused");
                    continue;
                }
                Event::ReachClosed => {
                    warn!("pinc hub adjacency table closed");
                    continue;
                }
                Event::ReachOpened => {
                    debug!("pinc hub adjacency table opened again");
                    continue;
                }
            };
            if self.events.send(event).await.is_err() {
                debug!("Event channel closed, pinc topic events stop");
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
        // who is connected to whom (pinc facade gap).
        if self.peers.borrow().is_empty() {
            debug!(topic, "pinc topic message dropped: this node has no peer");
            return None;
        }
        let origin_id = node_to_peer_id(&origin);
        let message_id = self.next_message_id();
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
            .inspect_err(|error| record_decode_failure(&origin_id, &topic, &payload, error))
            .ok()?;
        authenticate_origin(&mut broadcast, &origin_id)
            .inspect_err(|error| {
                warn!(peer_id = %origin_id, topic, %error, "dropping pinc head hint with an unauthenticated origin");
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

/// pinc verified `publisher`'s signature over the payload, so it is the
/// provider. A payload that names another origin must carry that origin's
/// signature.
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
        .ok_or_else(|| Error::InvalidPeerId(format!("not a pinc peer id: {claimed}")))?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&key).map_err(|_| Error::InvalidSignature)?;
    let signature =
        ed25519_dalek::Signature::from_slice(signature).map_err(|_| Error::InvalidSignature)?;
    key.verify(signed, &signature)
        .map_err(|_| Error::InvalidSignature)
}

/// Count a head hint that did not decode, keep a sample of it for the sync
/// diagnostics, and warn on the first and every doubling after.
fn record_decode_failure(origin: &PeerId, topic: &str, payload: &[u8], error: &str) {
    let info = PushLogBroadcast::inspect_gossip_payload(payload);
    let count =
        p2p::sync::record_gossip_decode_failure_sample(p2p::sync::GossipDecodeFailureSample {
            transport: p2p::sync::GossipTransport::Pinc,
            peer_id: origin.to_string(),
            topic: topic.to_owned(),
            message_size: payload.len(),
            error: error.to_owned(),
            payload_fingerprint: info.payload_fingerprint,
            payload_shape_hint: info.payload_shape_hint,
            occurrences: 0,
        });
    if count.is_power_of_two() {
        warn!(peer_id = %origin, topic, message_size = payload.len(), total_failures = count, error, "undecodable pinc head hint");
    } else {
        debug!(peer_id = %origin, topic, total_failures = count, error, "undecodable pinc head hint");
    }
}

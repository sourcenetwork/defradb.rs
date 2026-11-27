//! The task that owns this node's record and its book of other nodes'.
//!
//! It signs our record when what we offer changes or before it expires,
//! points to it from pkarr, announces it on the discovery topic, answers
//! peers asking for records, and asks a peer for its record when the peer's
//! pointer is ahead of our copy, looking connected peers' pointers up again
//! in rotation so a lost announcement is still recovered. It never dials: every send goes through the
//! endpoint's admission-checked paths.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::rt::time::Instant;
use bytes::Bytes;
use futures::FutureExt;
use iroh::{EndpointId, SecretKey};
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, warn};

use super::{
    advertised_key, blind, sign, verify, Book, CollectionKey, NetworkSecret, Pointer, Refresh,
    Role, Service, ServiceRecord, SignedRecord, MAX_RECORD_BYTES, MAX_SERVICES,
};
use crate::iroh::command::Reply;
use crate::iroh::mailbox::{Mailbox, Wake};

/// The gossip topic records are announced on.
pub const RECORDS_TOPIC: &str = "defra/service-records/1";
/// How long a record we sign stays valid.
const RECORD_TTL: Duration = Duration::from_secs(60 * 60);
/// Re-sign once less than this much validity is left.
const RENEW_BEFORE: Duration = Duration::from_secs(30 * 60);
/// The shortest gap between two announcements of our record.
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(30);
/// How often the task looks at its own record.
const TICK: Duration = Duration::from_secs(10);
/// The most records one reply carries.
pub const MAX_REPLY_RECORDS: usize = 16;
/// Other nodes' records we keep.
const BOOK_CAPACITY: usize = 4096;
/// Connected peers whose pointers are looked up again, one batch per tick.
const RESOLVES_PER_TICK: usize = 4;
/// Peers a collection join is pointed at.
const JOIN_HINTS: usize = 8;
/// Nodes a providers lookup returns.
const MAX_PROVIDERS: usize = 8;
const INBOX_CAPACITY: usize = 1024;

/// What this node offers besides the collections it subscribes to.
#[derive(Clone, Debug)]
pub struct ServiceProfile {
    /// Blinds collection keys; without it only public collections are advertised.
    pub secret: Option<NetworkSecret>,
    pub role: Role,
    /// Collections advertised by their plain id.
    pub public_collections: RapidHashSet<String>,
    /// Replicator, SE query, KMS, relay and custom services.
    pub offers: Vec<Service>,
}

/// What peers send each other on the records stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordsMessage {
    /// Send me `node`'s record.
    Ask {
        node: [u8; 32],
    },
    /// Send me records of nodes advertising `key`.
    Who {
        key: CollectionKey,
    },
    Records(Vec<SignedRecord>),
}

/// What the task needs from the endpoint. Every call returns at once.
pub trait Net {
    /// Announces `record` on [`RECORDS_TOPIC`].
    fn announce(&self, record: Bytes);
    /// Publishes `pointer` in our pkarr user data.
    fn set_pointer(&self, pointer: Pointer);
    /// Sends `message` to `peer` through admission.
    fn send(&self, peer: EndpointId, message: RecordsMessage);
    /// Looks up `peer`'s pointer; the answer comes back as [`Input::Pointer`].
    fn resolve(&self, peer: EndpointId);
    /// Points the gossip join of `topic` at `peers`.
    fn join_hint(&self, topic: String, peers: Vec<EndpointId>);
}

pub enum Input {
    Profile(ServiceProfile),
    Collection {
        id: String,
        held: bool,
    },
    Announced(Bytes),
    Message {
        from: EndpointId,
        message: RecordsMessage,
    },
    Connected(EndpointId),
    /// Our last connection to the peer closed.
    Disconnected(EndpointId),
    /// `None` when the peer publishes no pointer we can read.
    Pointer {
        node: EndpointId,
        pointer: Option<Pointer>,
    },
    /// The live record we hold for `node`, ours included.
    Lookup {
        node: EndpointId,
        reply: Reply<Option<ServiceRecord>>,
    },
    /// Nodes whose live record holds collection `id`, replicas first, then
    /// archives, then readers. When the book has none, `ask` is asked who
    /// holds it, so a later lookup can find them.
    Providers {
        id: String,
        ask: Vec<EndpointId>,
        reply: Reply<Vec<EndpointId>>,
    },
    /// Nodes that offer to be a replicator, holding collection `id` when given.
    Replicators {
        id: Option<String>,
        reply: Reply<Vec<EndpointId>>,
    },
}

/// The task's inputs in two lanes it waits on through one [`Wake`], the
/// shape `proofs/tla/GossipInbox.tla` checks: changes to what we offer and
/// lookups never wait, so the endpoint loop never waits on this task; hints
/// from peers are dropped when their lane is full.
#[derive(Clone)]
pub struct RecordsHandle {
    changes: Arc<Mailbox<Input>>,
    hints: Arc<Mailbox<Input>>,
    wake: Wake,
}

impl Default for RecordsHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordsHandle {
    pub fn new() -> Self {
        let wake = Wake::default();
        Self {
            changes: Arc::new(Mailbox::unbounded(wake.clone())),
            hints: Arc::new(Mailbox::new(INBOX_CAPACITY, wake.clone())),
            wake,
        }
    }

    /// Queues `input`, dropping it if the task is behind: every input is
    /// either repeated later (announcements, connections) or a hint.
    pub fn push(&self, input: Input) {
        if self.hints.try_push(input).is_err() {
            debug!("service records inbox full, input dropped");
        }
    }

    /// Queues a change to what we offer, or a lookup: neither is lost, and
    /// queueing never waits.
    pub fn push_change(&self, input: Input) {
        let _ = self.changes.try_push(input);
    }

    pub fn close(&self) {
        self.changes.close();
    }

    /// The next input, changes first; `None` once closed and empty.
    pub(super) async fn next(&self) -> Option<Input> {
        loop {
            // Read the flag before looking, so an input queued just before
            // the close is still returned.
            let closed = self.changes.is_closed();
            if let Some(input) = self.changes.pop() {
                return Some(input);
            }
            if let Some(input) = self.hints.pop() {
                return Some(input);
            }
            if closed {
                return None;
            }
            self.wake.notified().await;
        }
    }

    /// Runs the task until the handle is closed.
    pub async fn run<N>(&self, key: SecretKey, net: N)
    where
        N: Net,
    {
        let mut records = Records::new(key, net);
        loop {
            let input = futures::select! {
                input = self.next().fuse() => match input {
                    Some(input) => Some(input),
                    None => break,
                },
                () = crate::rt::time::sleep(TICK).fuse() => None,
            };
            let now = Instant::now();
            if let Some(input) = input {
                records.on_input(input, now, unix_now());
            }
            records.renew(now, unix_now());
            records.rotate(now);
        }
    }
}

pub struct Records<N> {
    me: EndpointId,
    key: SecretKey,
    profile: Option<ServiceProfile>,
    collections: RapidHashSet<String>,
    own: Option<SignedRecord>,
    own_seq: u64,
    own_expires: u64,
    /// What we offer changed since our record was signed.
    changed: bool,
    announced: Option<Instant>,
    /// When we last asked peers who holds each collection.
    asked_who: RapidHashMap<String, Instant>,
    book: Book,
    refresh: Refresh,
    /// Connected peers, in the order their pointers are looked up again.
    peers: VecDeque<EndpointId>,
    connected: RapidHashSet<EndpointId>,
    rotated: Option<Instant>,
    net: N,
}

impl<N> Records<N>
where
    N: Net,
{
    pub fn new(key: SecretKey, net: N) -> Self {
        Self {
            me: key.public(),
            key,
            profile: None,
            collections: RapidHashSet::new(),
            own: None,
            own_seq: 0,
            own_expires: 0,
            changed: false,
            announced: None,
            asked_who: RapidHashMap::new(),
            book: Book::new(BOOK_CAPACITY),
            refresh: Refresh::new(),
            peers: VecDeque::new(),
            connected: RapidHashSet::new(),
            rotated: None,
            net,
        }
    }

    pub fn on_input(&mut self, input: Input, now: Instant, unix: u64) {
        match input {
            Input::Profile(profile) => {
                self.profile = Some(profile);
                self.changed = true;
            }
            Input::Collection { id, held } => {
                let changed = if held {
                    self.collections.insert(id.clone())
                } else {
                    self.collections.remove(&id)
                };
                self.changed |= changed;
                if held {
                    self.hint_join(id, unix);
                }
            }
            Input::Announced(bytes) => match postcard::from_bytes::<SignedRecord>(&bytes) {
                Ok(signed) => self.offer(signed, unix),
                Err(_) => debug!("undecodable service record announcement"),
            },
            Input::Message { from, message } => self.on_message(from, message, unix),
            Input::Connected(peer) => {
                self.net.resolve(peer);
                if self.connected.len() < BOOK_CAPACITY && self.connected.insert(peer) {
                    self.peers.push_back(peer);
                }
            }
            Input::Disconnected(peer) => {
                if self.connected.remove(&peer) {
                    self.peers.retain(|known| *known != peer);
                }
            }
            Input::Lookup { node, reply } => {
                let record = if node == self.me {
                    self.own.as_ref().and_then(|own| verify(own, unix).ok())
                } else {
                    self.book.live(&node, unix).cloned()
                };
                let _ = reply.send(Ok(record));
            }
            Input::Providers { id, ask, reply } => {
                let _ = reply.send(Ok(self.providers(&id, &ask, now, unix)));
            }
            Input::Replicators { id, reply } => {
                let _ = reply.send(Ok(self.replicators(id.as_deref(), unix)));
            }
            Input::Pointer { node, pointer } => {
                // A peer with no readable pointer is asked only while we hold
                // nothing live for it.
                let pointer = match pointer {
                    Some(pointer) => pointer,
                    None if self.book.live(&node, unix).is_none() => Pointer { seq: 0, hash: 0 },
                    None => return,
                };
                if node != self.me && self.refresh.check(&self.book, node, &pointer, now, unix) {
                    self.net.send(
                        node,
                        RecordsMessage::Ask {
                            node: *node.as_bytes(),
                        },
                    );
                }
            }
        }
    }

    /// Signs and announces our record when what we offer changed or it is
    /// close to expiring, at most once per announce interval.
    pub fn renew(&mut self, now: Instant, unix: u64) {
        let Some(profile) = &self.profile else {
            return;
        };
        let expiring = self.own.is_some() && unix + RENEW_BEFORE.as_secs() >= self.own_expires;
        if !self.changed && !expiring && self.own.is_some() {
            return;
        }
        if self
            .announced
            .is_some_and(|last| now.duration_since(last) < ANNOUNCE_INTERVAL)
        {
            return;
        }
        let seq = (unix_millis()).max(self.own_seq + 1);
        let expires_at = unix + RECORD_TTL.as_secs();
        let record = build(self.me, seq, expires_at, profile, &self.collections);
        let signed = sign(&self.key, &record);
        self.net.set_pointer(Pointer {
            seq,
            hash: signed.hash(),
        });
        self.net.announce(Bytes::from(
            postcard::to_allocvec(&signed).expect("a signed record always encodes"),
        ));
        self.own = Some(signed);
        self.own_seq = seq;
        self.own_expires = expires_at;
        self.changed = false;
        self.announced = Some(now);
    }

    /// Looks up the next few connected peers' pointers, once per tick. An
    /// announcement gossip lost is then still recovered on a long-lived
    /// connection (`ServiceRecords.tla`, `LIVE_NewestReached`).
    pub fn rotate(&mut self, now: Instant) {
        if self
            .rotated
            .is_some_and(|last| now.duration_since(last) < TICK)
        {
            return;
        }
        self.rotated = Some(now);
        for _ in 0..RESOLVES_PER_TICK.min(self.peers.len()) {
            if let Some(peer) = self.peers.pop_front() {
                self.net.resolve(peer);
                self.peers.push_back(peer);
            }
        }
    }

    fn on_message(&mut self, from: EndpointId, message: RecordsMessage, unix: u64) {
        match message {
            RecordsMessage::Ask { node } => {
                let record = if node == *self.me.as_bytes() {
                    self.own.clone()
                } else {
                    EndpointId::from_bytes(&node)
                        .ok()
                        .and_then(|node| self.book.signed(&node, unix).cloned())
                };
                if let Some(record) = record {
                    self.net.send(from, RecordsMessage::Records(vec![record]));
                }
            }
            RecordsMessage::Who { key } => {
                let own = self
                    .own
                    .as_ref()
                    .filter(|own| verify(own, unix).is_ok_and(|record| holds(&record, &key)));
                let records: Vec<SignedRecord> = own
                    .cloned()
                    .into_iter()
                    .chain(
                        self.book
                            .who(&key, unix, MAX_REPLY_RECORDS)
                            .iter()
                            .filter_map(|node| self.book.signed(node, unix).cloned()),
                    )
                    .take(MAX_REPLY_RECORDS)
                    .collect();
                if !records.is_empty() {
                    self.net.send(from, RecordsMessage::Records(records));
                }
            }
            RecordsMessage::Records(records) => {
                for signed in records.into_iter().take(MAX_REPLY_RECORDS) {
                    self.offer(signed, unix);
                }
            }
        }
    }

    fn offer(&mut self, signed: SignedRecord, unix: u64) {
        if let Err(rejection) = self.book.offer(signed, unix) {
            debug!(%rejection, "service record refused");
        }
    }

    /// Collection `id`'s keys: blinded, then plain (a peer may have made it
    /// public).
    fn keys(&self, id: &str) -> Vec<CollectionKey> {
        let secret = self
            .profile
            .as_ref()
            .and_then(|profile| profile.secret.as_ref());
        secret
            .map(|secret| blind(secret, id))
            .into_iter()
            .chain([CollectionKey::Plain(id.to_string())])
            .collect()
    }

    fn providers(
        &mut self,
        id: &str,
        ask: &[EndpointId],
        now: Instant,
        unix: u64,
    ) -> Vec<EndpointId> {
        let keys = self.keys(id);
        let mut ranked: Vec<(u8, EndpointId)> = Vec::new();
        for key in &keys {
            for node in self.book.who(key, unix, BOOK_CAPACITY) {
                let role = self
                    .book
                    .live(&node, unix)
                    .and_then(|record| role_of(record, key));
                if let Some(role) = role {
                    if node != self.me && !ranked.iter().any(|(_, seen)| *seen == node) {
                        ranked.push((rank(role), node));
                    }
                }
            }
        }
        let asked_lately = self
            .asked_who
            .get(id)
            .is_some_and(|at| now.duration_since(*at) < ANNOUNCE_INTERVAL);
        if ranked.is_empty() && !asked_lately && !ask.is_empty() {
            self.asked_who
                .retain(|_, at| now.duration_since(*at) < ANNOUNCE_INTERVAL);
            self.asked_who.insert(id.to_string(), now);
            for peer in ask {
                for key in &keys {
                    self.net
                        .send(*peer, RecordsMessage::Who { key: key.clone() });
                }
            }
        }
        ranked.sort_by_key(|(rank, _)| *rank);
        ranked
            .into_iter()
            .take(MAX_PROVIDERS)
            .map(|(_, node)| node)
            .collect()
    }

    fn replicators(&self, id: Option<&str>, unix: u64) -> Vec<EndpointId> {
        let keys = id.map(|id| self.keys(id));
        self.book
            .live_records(unix)
            .filter(|record| record.services.contains(&Service::Replicator))
            .filter(|record| {
                keys.as_ref()
                    .is_none_or(|keys| keys.iter().any(|key| holds(record, key)))
            })
            .filter_map(|record| EndpointId::from_bytes(&record.node).ok())
            .filter(|node| *node != self.me)
            .collect()
    }

    /// Points the join of collection `id` at nodes that advertise it.
    fn hint_join(&mut self, id: String, unix: u64) {
        let mut peers = Vec::new();
        for key in self.keys(&id) {
            for node in self.book.who(&key, unix, JOIN_HINTS) {
                if node != self.me && !peers.contains(&node) && peers.len() < JOIN_HINTS {
                    peers.push(node);
                }
            }
        }
        if !peers.is_empty() {
            self.net.join_hint(id, peers);
        }
    }
}

/// Our record: what the profile offers, then as many held collections as
/// fit the service count and the body size a reader accepts.
fn build(
    me: EndpointId,
    seq: u64,
    expires_at: u64,
    profile: &ServiceProfile,
    collections: &RapidHashSet<String>,
) -> ServiceRecord {
    let mut record = ServiceRecord {
        node: *me.as_bytes(),
        seq,
        expires_at,
        services: profile.offers.iter().take(MAX_SERVICES).cloned().collect(),
    };
    let mut ids: Vec<&String> = collections.iter().collect();
    ids.sort();
    let mut left_out = 0;
    for id in ids {
        let public = profile.public_collections.contains(id.as_str());
        let Some(key) = advertised_key(profile.secret.as_ref(), public, id) else {
            continue;
        };
        record.services.push(Service::Collection {
            key,
            role: profile.role,
        });
        let fits = record.services.len() <= MAX_SERVICES
            && postcard::to_allocvec(&record).is_ok_and(|body| body.len() <= MAX_RECORD_BYTES);
        if !fits {
            record.services.pop();
            left_out += 1;
        }
    }
    if left_out > 0 {
        warn!(
            left_out,
            "service record full: some collections are not advertised"
        );
    }
    record
}

fn unix_now() -> u64 {
    unix_millis() / 1000
}

fn unix_millis() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// The role `record` holds collection `key` in, if it holds it.
fn role_of(record: &ServiceRecord, key: &CollectionKey) -> Option<Role> {
    record.services.iter().find_map(|service| match service {
        Service::Collection { key: held, role } if held == key => Some(*role),
        _ => None,
    })
}

fn holds(record: &ServiceRecord, key: &CollectionKey) -> bool {
    role_of(record, key).is_some()
}

/// Replicas keep a collection current; archives may lag; readers hold only
/// what they read.
fn rank(role: Role) -> u8 {
    match role {
        Role::Replica => 0,
        Role::Archive => 1,
        Role::Reader => 2,
    }
}

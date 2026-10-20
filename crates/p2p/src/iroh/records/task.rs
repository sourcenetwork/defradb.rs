//! The task that owns this node's record and its book of other nodes'.
//!
//! It signs our record when what we offer changes or before it expires,
//! points to it from pkarr, announces it on the discovery topic, answers
//! peers asking for records, and asks a peer for its record when the peer's
//! pointer is ahead of our copy. It never dials: every send goes through the
//! endpoint's admission-checked paths.

use std::sync::Arc;

use bytes::Bytes;
use iroh::{EndpointId, SecretKey};
use n0_future::time::{Duration, Instant};
use rapidhash::{HashSetExt, RapidHashSet};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tracing::{debug, warn};

use super::{
    advertised_key, blind, sign, verify, Book, CollectionKey, NetworkSecret, Pointer, Refresh,
    Role, Service, ServiceRecord, SignedRecord, MAX_RECORD_BYTES, MAX_SERVICES,
};
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
/// Peers a collection join is pointed at.
const JOIN_HINTS: usize = 8;
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
    /// `None` when the peer publishes no pointer we can read.
    Pointer {
        node: EndpointId,
        pointer: Option<Pointer>,
    },
    /// The live record we hold for `node`, ours included.
    Lookup {
        node: EndpointId,
        reply: oneshot::Sender<crate::error::Result<Option<ServiceRecord>>>,
    },
}

#[derive(Clone)]
pub struct RecordsHandle {
    inbox: Arc<Mailbox<Input>>,
}

impl Default for RecordsHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordsHandle {
    pub fn new() -> Self {
        Self {
            inbox: Arc::new(Mailbox::new(INBOX_CAPACITY, Wake::default())),
        }
    }

    /// Queues `input`, dropping it if the task is behind: every input is
    /// either repeated later (announcements, connections) or a hint.
    pub fn push(&self, input: Input) {
        if self.inbox.try_push(input).is_err() {
            debug!("service records inbox full, input dropped");
        }
    }

    /// Queues a change to what we offer, waiting for room: it must not be lost.
    pub async fn push_change(&self, input: Input) {
        self.inbox.push(input).await;
    }

    pub fn close(&self) {
        self.inbox.close();
    }

    /// Runs the task until the handle is closed.
    pub async fn run<N>(&self, key: SecretKey, net: N)
    where
        N: Net,
    {
        let mut records = Records::new(key, net);
        loop {
            let input = tokio::select! {
                input = self.inbox.next() => match input {
                    Some(input) => Some(input),
                    None => break,
                },
                () = n0_future::time::sleep(TICK) => None,
            };
            let now = Instant::now();
            if let Some(input) = input {
                records.on_input(input, now, unix_now());
            }
            records.renew(now, unix_now());
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
    book: Book,
    refresh: Refresh,
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
            book: Book::new(BOOK_CAPACITY),
            refresh: Refresh::new(),
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
            Input::Connected(peer) => self.net.resolve(peer),
            Input::Lookup { node, reply } => {
                let record = if node == self.me {
                    self.own.as_ref().and_then(|own| verify(own, unix).ok())
                } else {
                    self.book.live(&node, unix).cloned()
                };
                let _ = reply.send(Ok(record));
            }
            Input::Pointer { node, pointer } => {
                // A peer with no readable pointer is asked only while we hold
                // nothing live for it.
                let pointer = match pointer {
                    Some(pointer) => pointer,
                    None if self.book.live(&node, unix).is_none() => Pointer { seq: 0, hash: 0 },
                    None => return,
                };
                if node != self.me && self.refresh.check(&self.book, node, &pointer, now) {
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
                let records: Vec<SignedRecord> = self
                    .book
                    .who(&key, unix, MAX_REPLY_RECORDS)
                    .iter()
                    .filter_map(|node| self.book.signed(node, unix).cloned())
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

    /// Points the join of collection `id` at nodes that advertise it, under
    /// its blinded key and its plain one (a peer may have made it public).
    fn hint_join(&mut self, id: String, unix: u64) {
        let mut peers = Vec::new();
        let secret = self
            .profile
            .as_ref()
            .and_then(|profile| profile.secret.as_ref());
        let keys = secret
            .map(|secret| blind(secret, &id))
            .into_iter()
            .chain([CollectionKey::Plain(id.clone())]);
        for key in keys {
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

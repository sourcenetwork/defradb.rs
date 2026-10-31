//! Go Shinzo's gossip batches, both ways: CARs that arrive inline with an
//! update, kept until the coordinator asks for them, and this node's own
//! collection-topic updates, published in Go's `documents` shape.
//!
//! Both are owned by the host task alone, so they are plain owned values.
//! Go: shinzo-develop `internal/db/p2p/batcher.go`, `p2p.go` processPushlogRequest.

use std::time::{Duration, Instant};

use bytes::Bytes;
use cid::Cid;
use rapidhash::RapidHashMap;

use crate::message::{DocumentInfo, PushLogRequest};
use crate::sync::shinzo_car::car_rooted_at;

#[cfg(test)]
#[path = "../../tests/unit/shinzo_batch.rs"]
mod tests;

/// The most CAR bytes kept for heads not yet asked for.
pub const INLINE_CAR_BUDGET: usize = 64 << 20;
/// How long an inline CAR waits for the coordinator to ask for its head.
pub const INLINE_CAR_TTL: Duration = Duration::from_secs(60);

/// CARs that came inline with an update (a Go peer predating the `car`
/// channel sends them), by head and the authenticated peer that offered
/// them. Taken, at most once, when the coordinator asks that peer for that
/// head, in place of a network request, so no other peer can replace or
/// spend an honest peer's CAR.
#[derive(Debug, Default)]
pub struct InlineCars {
    cars: RapidHashMap<(Cid, String), (Bytes, Instant)>,
    bytes: usize,
}

impl InlineCars {
    /// Keep `car` for `head` as offered by `peer`, if it is a CAR whose only
    /// root is `head` and it fits the budget once CARs older than the TTL are
    /// dropped.
    pub fn offer(&mut self, head: Cid, peer: &str, car: Bytes, now: Instant) -> bool {
        if !car_rooted_at(&car, &head) {
            return false;
        }
        self.expire(now);
        if self.bytes + car.len() > INLINE_CAR_BUDGET {
            return false;
        }
        self.bytes += car.len();
        if let Some((old, _)) = self.cars.insert((head, peer.to_string()), (car, now)) {
            self.bytes -= old.len();
        }
        true
    }

    /// The CAR `peer` offered for `head`, left in place.
    pub fn peek(&self, head: &Cid, peer: &str) -> Option<Bytes> {
        self.cars
            .get(&(*head, peer.to_string()))
            .map(|(car, _)| car.clone())
    }

    /// The CAR `peer` offered for `head`, removed so it is used once.
    pub fn take(&mut self, head: &Cid, peer: &str) -> Option<Bytes> {
        let (car, _) = self.cars.remove(&(*head, peer.to_string()))?;
        self.bytes -= car.len();
        Some(car)
    }

    fn expire(&mut self, now: Instant) {
        let bytes = &mut self.bytes;
        self.cars.retain(|_, (car, at)| {
            let keep = now.duration_since(*at) < INLINE_CAR_TTL;
            if !keep {
                *bytes -= car.len();
            }
            keep
        });
    }
}

/// Go's `batchMaxDocs`: a topic's batch is published at once when this full.
pub const BATCH_MAX_DOCS: usize = 64;
/// Go's `batchFlushInterval`: a topic's batch waits at most this long.
pub const BATCH_FLUSH_INTERVAL: Duration = Duration::from_millis(50);
/// Go's `maxPubsubMessageSize`: an estimated message larger than this is
/// split in half until each part fits.
pub const MAX_BATCH_MESSAGE_BYTES: usize = 1000 * 1024;
/// How many times a batch is tried before it is dropped, as a single
/// update's gossip is (the broadcast mutator's `BROADCAST_MAX_RETRIES`).
/// Gossip is not the record of delivery: durable replicator markers and
/// document sync still carry the documents.
pub const BATCH_MAX_PUBLISH_ATTEMPTS: u32 = 10;
/// The longest a failed batch waits before it is tried again.
pub const BATCH_MAX_RETRY_DELAY: Duration = Duration::from_secs(10);

/// One topic's documents, ready to publish, and how many times publishing
/// them has failed so far.
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub topic: String,
    pub documents: Vec<DocumentInfo>,
    pub failures: u32,
}

/// This node's collection-topic updates, gathered per topic as Go's
/// `pubsubBatcher` does, and batches waiting to be tried again.
#[derive(Debug, Default)]
pub struct ShinzoBatcher {
    pending: RapidHashMap<String, (Instant, Vec<DocumentInfo>)>,
    retries: Vec<(Instant, Batch)>,
}

impl ShinzoBatcher {
    /// Add `document` to `topic`'s batch; the batch comes back when full.
    pub fn add(&mut self, topic: &str, document: DocumentInfo, now: Instant) -> Option<Batch> {
        let (_, documents) = self
            .pending
            .entry(topic.to_string())
            .or_insert_with(|| (now, Vec::new()));
        documents.push(document);
        if documents.len() < BATCH_MAX_DOCS {
            return None;
        }
        self.pending
            .remove_entry(topic)
            .map(|(topic, (_, documents))| Batch {
                topic,
                documents,
                failures: 0,
            })
    }

    /// Keep a batch whose publish failed (a retryable failure) to try again
    /// after a backoff that doubles with each failure. `false` once it has
    /// failed `BATCH_MAX_PUBLISH_ATTEMPTS` times: the caller drops it.
    pub fn retry(&mut self, mut batch: Batch, now: Instant) -> bool {
        batch.failures += 1;
        if batch.failures >= BATCH_MAX_PUBLISH_ATTEMPTS {
            return false;
        }
        let delay = BATCH_FLUSH_INTERVAL
            .saturating_mul(1 << batch.failures.min(16))
            .min(BATCH_MAX_RETRY_DELAY);
        self.retries.push((now + delay, batch));
        true
    }

    /// The batches whose first document waited `BATCH_FLUSH_INTERVAL`, and
    /// the failed batches whose backoff has passed.
    pub fn due(&mut self, now: Instant) -> Vec<Batch> {
        let topics: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, (since, _))| now.duration_since(*since) >= BATCH_FLUSH_INTERVAL)
            .map(|(topic, _)| topic.clone())
            .collect();
        let mut due: Vec<Batch> = topics
            .into_iter()
            .filter_map(|topic| {
                self.pending
                    .remove_entry(&topic)
                    .map(|(topic, (_, documents))| Batch {
                        topic,
                        documents,
                        failures: 0,
                    })
            })
            .collect();
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.retries)
            .into_iter()
            .partition(|(at, _)| *at <= now);
        self.retries = waiting;
        due.extend(ready.into_iter().map(|(_, batch)| batch));
        due
    }

    /// Every batch, retries included, for shutdown.
    pub fn drain(&mut self) -> Vec<Batch> {
        let mut all: Vec<Batch> = self
            .pending
            .drain()
            .map(|(topic, (_, documents))| Batch {
                topic,
                documents,
                failures: 0,
            })
            .collect();
        all.extend(self.retries.drain(..).map(|(_, batch)| batch));
        all
    }

    /// Whether any batch is waiting, to gather or to try again.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.retries.is_empty()
    }
}

/// One encoded batch message and the documents it carries.
pub type EncodedBatch = (Vec<u8>, Vec<DocumentInfo>);

/// Encode `documents` for `topic` as Go's batcher does: a pushlog with an
/// empty envelope, the topic as its collection, this node as creator, and
/// the documents; halves are encoded apart while Go's size estimate is over
/// `MAX_BATCH_MESSAGE_BYTES`. Each message comes back with the documents it
/// carries, so a message that fails to publish can be tried again alone.
pub fn encode_batch(
    topic: &str,
    creator: &str,
    documents: Vec<DocumentInfo>,
) -> Result<Vec<EncodedBatch>, defra_core::cbor::Error> {
    let estimate: usize = documents
        .iter()
        .map(|d| d.doc_id.len() + d.cid.len() + d.block.len() + d.car.len() + 50)
        .sum();
    if estimate > MAX_BATCH_MESSAGE_BYTES && documents.len() > 1 {
        let mut first = documents;
        let second = first.split_off(first.len() / 2);
        let mut messages = encode_batch(topic, creator, first)?;
        messages.extend(encode_batch(topic, creator, second)?);
        return Ok(messages);
    }
    let mut request = PushLogRequest::new(
        String::new(),
        Bytes::new(),
        topic.to_string(),
        creator.to_string(),
        Bytes::new(),
    );
    request.version = String::new();
    request.documents = documents;
    let data = defra_core::cbor::to_vec(&request)?;
    Ok(vec![(data, request.documents)])
}

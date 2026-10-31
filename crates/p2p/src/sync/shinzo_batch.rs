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
/// channel sends them), by head. Taken, at most once, when the coordinator
/// asks for that head, in place of a network request.
#[derive(Debug, Default)]
pub struct InlineCars {
    cars: RapidHashMap<Cid, (Bytes, Instant)>,
    bytes: usize,
}

impl InlineCars {
    /// Keep `car` for `head`, if it is a CAR whose only root is `head` and
    /// it fits the budget once CARs older than the TTL are dropped.
    pub fn offer(&mut self, head: Cid, car: Bytes, now: Instant) -> bool {
        if !car_rooted_at(&car, &head) {
            return false;
        }
        self.expire(now);
        if self.bytes + car.len() > INLINE_CAR_BUDGET {
            return false;
        }
        self.bytes += car.len();
        if let Some((old, _)) = self.cars.insert(head, (car, now)) {
            self.bytes -= old.len();
        }
        true
    }

    /// The CAR kept for `head`, removed so it is used once.
    pub fn take(&mut self, head: &Cid) -> Option<Bytes> {
        let (car, _) = self.cars.remove(head)?;
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

/// This node's collection-topic updates, gathered per topic as Go's
/// `pubsubBatcher` does.
#[derive(Debug, Default)]
pub struct ShinzoBatcher {
    pending: RapidHashMap<String, (Instant, Vec<DocumentInfo>)>,
}

impl ShinzoBatcher {
    /// Add `document` to `topic`'s batch; the batch comes back when full.
    pub fn add(
        &mut self,
        topic: &str,
        document: DocumentInfo,
        now: Instant,
    ) -> Option<Vec<DocumentInfo>> {
        let (_, documents) = self
            .pending
            .entry(topic.to_string())
            .or_insert_with(|| (now, Vec::new()));
        documents.push(document);
        if documents.len() < BATCH_MAX_DOCS {
            return None;
        }
        self.pending.remove(topic).map(|(_, documents)| documents)
    }

    /// The batches whose first document waited `BATCH_FLUSH_INTERVAL`.
    pub fn due(&mut self, now: Instant) -> Vec<(String, Vec<DocumentInfo>)> {
        let topics: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, (since, _))| now.duration_since(*since) >= BATCH_FLUSH_INTERVAL)
            .map(|(topic, _)| topic.clone())
            .collect();
        topics
            .into_iter()
            .filter_map(|topic| self.pending.remove(&topic).map(|(_, docs)| (topic, docs)))
            .collect()
    }

    /// Every batch, for shutdown.
    pub fn drain(&mut self) -> Vec<(String, Vec<DocumentInfo>)> {
        self.pending
            .drain()
            .map(|(topic, (_, documents))| (topic, documents))
            .collect()
    }

    /// Whether any batch is waiting.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// Encode `documents` for `topic` as Go's batcher does: a pushlog with an
/// empty envelope, the topic as its collection, this node as creator, and
/// the documents; halves are encoded apart while Go's size estimate is over
/// `MAX_BATCH_MESSAGE_BYTES`.
pub fn encode_batch(
    topic: &str,
    creator: &str,
    documents: Vec<DocumentInfo>,
) -> Result<Vec<Vec<u8>>, defra_core::cbor::Error> {
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
    Ok(vec![defra_core::cbor::to_vec(&request)?])
}

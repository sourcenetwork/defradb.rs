//! The newest valid record of each node we have heard from, bounded.
//! Proofs: `Book::offer` and `Book::live` follow `ServiceRecords.tla` and
//! `proofs/lean/ServiceRecord/Validation.lean`.

use iroh::EndpointId;
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};

use super::{verify, CollectionKey, Pointer, Rejection, Service, ServiceRecord, SignedRecord};

pub struct Book {
    held: RapidHashMap<EndpointId, Held>,
    /// Nodes advertising each collection key, by the key's encoding.
    by_key: RapidHashMap<Vec<u8>, RapidHashSet<EndpointId>>,
    capacity: usize,
}

struct Held {
    record: ServiceRecord,
    signed: SignedRecord,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Offered {
    Taken,
    /// We already hold this node's live record at an equal or higher seq.
    Stale,
    /// The book is full of live records. Records are self-signed and node
    /// keys are free, so an unsolicited record never displaces a live one.
    Full,
}

impl Book {
    pub fn new(capacity: usize) -> Self {
        Self {
            held: RapidHashMap::new(),
            by_key: RapidHashMap::new(),
            capacity,
        }
    }

    /// Keeps `signed` if it is valid at `now` and newer than the live record
    /// we hold for its node. A full book drops expired records to make room.
    pub fn offer(&mut self, signed: SignedRecord, now: u64) -> Result<Offered, Rejection> {
        let record = verify(&signed, now)?;
        let node = EndpointId::from_bytes(&record.node).map_err(|_| Rejection::BadSignature)?;
        if self
            .live(&node, now)
            .is_some_and(|old| old.seq >= record.seq)
        {
            return Ok(Offered::Stale);
        }
        self.remove(&node);
        if self.held.len() >= self.capacity {
            self.drop_expired(now);
        }
        if self.held.len() >= self.capacity {
            return Ok(Offered::Full);
        }
        for key in collection_keys(&record) {
            self.by_key.entry(key.encode()).or_default().insert(node);
        }
        self.held.insert(node, Held { record, signed });
        Ok(Offered::Taken)
    }

    /// `node`'s record, only while it has not expired.
    pub fn live(&self, node: &EndpointId, now: u64) -> Option<&ServiceRecord> {
        self.held
            .get(node)
            .map(|held| &held.record)
            .filter(|record| now < record.expires_at)
    }

    /// The signed record as we received it, to serve to a querying peer.
    pub fn signed(&self, node: &EndpointId, now: u64) -> Option<&SignedRecord> {
        self.live(node, now)?;
        self.held.get(node).map(|held| &held.signed)
    }

    /// Nodes whose live record advertises `key`, at most `limit` of them.
    pub fn who(&self, key: &CollectionKey, now: u64, limit: usize) -> Vec<EndpointId> {
        let Some(nodes) = self.by_key.get(&key.encode()) else {
            return Vec::new();
        };
        nodes
            .iter()
            .filter(|node| self.live(node, now).is_some())
            .take(limit)
            .copied()
            .collect()
    }

    /// Every live record we hold.
    pub fn live_records(&self, now: u64) -> impl Iterator<Item = &ServiceRecord> {
        self.held
            .values()
            .map(|held| &held.record)
            .filter(move |record| now < record.expires_at)
    }

    /// Whether `pointer` names a newer record than the live one we hold.
    pub fn is_behind(&self, node: &EndpointId, pointer: &Pointer, now: u64) -> bool {
        match self.live(node, now) {
            Some(record) => pointer.seq > record.seq,
            None => true,
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.held.len()
    }

    fn drop_expired(&mut self, now: u64) {
        let expired: Vec<EndpointId> = self
            .held
            .iter()
            .filter(|(_, held)| now >= held.record.expires_at)
            .map(|(node, _)| *node)
            .collect();
        for node in &expired {
            self.remove(node);
        }
    }

    fn remove(&mut self, node: &EndpointId) {
        let Some(held) = self.held.remove(node) else {
            return;
        };
        for key in collection_keys(&held.record) {
            let encoded = key.encode();
            if let Some(nodes) = self.by_key.get_mut(&encoded) {
                nodes.remove(node);
                if nodes.is_empty() {
                    self.by_key.remove(&encoded);
                }
            }
        }
    }
}

fn collection_keys(record: &ServiceRecord) -> impl Iterator<Item = &CollectionKey> {
    record.services.iter().filter_map(|service| match service {
        Service::Collection { key, .. } => Some(key),
        _ => None,
    })
}

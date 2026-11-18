//! Signed, expiring records of the services a node offers.
//!
//! A node signs one record listing what it offers (collections and their
//! role, replicator, SE query, KMS, relay, custom services), points to it
//! from its pkarr entry, serves it on a query stream and announces changes
//! on a discovery topic. Readers keep the newest valid copy per node in a
//! [`Book`] and pick gossip neighbours and fetch candidates from it.
//!
//! Proofs: `proofs/tla/ServiceRecords.tla` (replacement and refresh) and
//! `proofs/lean/ServiceRecord` (validation, key blinding, pointer encoding).

mod book;
mod key;
mod pointer;
mod refresh;
mod task;
mod verify;

pub use book::Book;
pub use key::{advertised_key, blind, CollectionKey, NetworkSecret};
pub use pointer::Pointer;
pub use refresh::Refresh;
pub use task::{
    Input, Net, RecordsHandle, RecordsMessage, ServiceProfile, MAX_REPLY_RECORDS, RECORDS_TOPIC,
};
pub use verify::{sign, verify, Rejection, SignedRecord};

use serde::{Deserialize, Serialize};

/// The longest encoded record body accepted, checked before decoding. It
/// leaves room for the signature and gossip's envelope in a 4096-byte frame.
pub const MAX_RECORD_BYTES: usize = 3072;
/// The most services one record may list.
pub const MAX_SERVICES: usize = 64;
/// The longest name: a custom service, a relay URL or a plain collection id.
pub const MAX_NAME_BYTES: usize = 128;
/// The furthest into the future a record may expire, in seconds: the hour
/// a node signs for, plus an hour of clock skew. Records filling a book
/// then hold it no longer than an honest record would.
pub const MAX_TTL_SECS: u64 = 2 * 60 * 60;

/// What a node says it offers, signed by that node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRecord {
    /// The signer's endpoint key; the signature must verify against it.
    pub node: [u8; 32],
    /// Higher is newer. The signing time in milliseconds, kept increasing.
    pub seq: u64,
    /// Unix seconds after which the record must not be used.
    pub expires_at: u64,
    pub services: Vec<Service>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Service {
    Collection { key: CollectionKey, role: Role },
    Replicator,
    SeQuery,
    Kms,
    Relay { url: String },
    Custom { name: String },
}

/// How a node holds a collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// Keeps the collection current and serves it.
    Replica,
    /// Keeps history for restores; may lag.
    Archive,
    /// Queries the collection; holds what it has read.
    Reader,
}

impl Service {
    /// The name this service carries, if any, for the name limit.
    fn name(&self) -> Option<&str> {
        match self {
            Service::Collection {
                key: CollectionKey::Plain(id),
                ..
            } => Some(id),
            Service::Relay { url } => Some(url),
            Service::Custom { name } => Some(name),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/iroh/records.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/iroh/records_task.rs"]
mod task_tests;

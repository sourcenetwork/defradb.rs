//! The retention floor for replicated documents (Go `shinzo-develop`'s
//! `RetentionRule`, #5288). A node given a rule refuses, at merge, a
//! replicated document whose height field is at or below its collection's
//! floor, or has no valid height. Local writes, deleted documents and
//! branchable collections are never judged. `proofs/tla/Retention.tla`
//! models it.

use document::{Document, NormalValue};
use rapidhash::RapidHashMap;

/// The height field and floor a rule names for a collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetentionFloor {
    /// The integer field that holds a document's height.
    pub field: String,
    /// Documents at or below this height are refused. A floor of zero or
    /// less refuses nothing by height.
    pub floor: i64,
}

/// Why a replicated document was refused. Both read as
/// `<reason>: refused by the retention rule`, as Go's wrapped errors do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RetentionRefusal {
    #[error("replicated document is at or below the retention floor: refused by the retention rule")]
    BelowFloor,
    #[error("replicated document has no valid retention height: refused by the retention rule")]
    NoHeight,
}

/// Decides which collections are judged, and how. Called for every
/// replicated document on the merge path, so it must not block.
pub trait RetentionRule: Send + Sync {
    /// The field and floor for the collection with `collection_id`, or
    /// `None` when the rule does not cover it.
    fn retention_floor(&self, collection_id: &str) -> Option<RetentionFloor>;
}

/// A rule from a fixed map of collection IDs to their floors, as a node's
/// configuration gives it.
#[derive(Clone, Debug, Default)]
pub struct StaticRetentionRule {
    floors: RapidHashMap<String, RetentionFloor>,
}

impl StaticRetentionRule {
    /// A rule covering each collection ID in `floors`.
    pub fn new(floors: RapidHashMap<String, RetentionFloor>) -> Self {
        Self { floors }
    }
}

impl RetentionRule for StaticRetentionRule {
    fn retention_floor(&self, collection_id: &str) -> Option<RetentionFloor> {
        self.floors.get(collection_id).cloned()
    }
}

impl RetentionFloor {
    /// The refusal for `doc` as merged, or `None` when it is kept. Go's
    /// `CheckRetention`.
    pub fn refusal(&self, doc: &Document) -> Option<RetentionRefusal> {
        let height = match doc.get(&self.field) {
            Some(NormalValue::Int(height)) | Some(NormalValue::NillableInt(Some(height))) => {
                *height
            }
            _ => return Some(RetentionRefusal::NoHeight),
        };
        if height < 0 {
            return Some(RetentionRefusal::NoHeight);
        }
        (self.floor > 0 && height <= self.floor).then_some(RetentionRefusal::BelowFloor)
    }
}

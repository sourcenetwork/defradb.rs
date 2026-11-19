//! The store's prefix extractor: a prefix seek of one entity's keys reads
//! only the tables that hold that entity.
//!
//! Every table regolith writes keeps a bloom filter of the entity prefixes
//! its keys carry, as `families` cuts them: one document's commit priority
//! index and head publications; one collection's document values, deletion
//! markers and versions; one index's entries; one ACP object's tuples. A read-only scan
//! of exactly a prefix seeks it as a prefix (`iterator`), and regolith passes
//! over every table whose filter lacks the entity the prefix names. A
//! transaction's scans are read through its cursor, which regolith positions
//! without prefix filters, so they read as before.

use regolith::PrefixExtractor;

use super::families;

pub(super) struct DefraPrefixExtractor;

impl PrefixExtractor for DefraPrefixExtractor {
    fn extract<'a>(&self, key: &'a [u8]) -> Option<&'a [u8]> {
        key.get(..families::entity_len(key)?)
    }

    /// Any prefix that reaches the end of its entity names that entity: a
    /// key that starts with the prefix has the same entity, since where an
    /// entity ends is decided by the bytes before its end.
    fn extract_query<'a>(&self, prefix: &'a [u8]) -> Option<&'a [u8]> {
        self.extract(prefix)
    }

    /// Recorded in every table it builds. A change to what `families` cuts
    /// takes a new name, so tables built under the old cuts are read without
    /// their prefix filter until a compaction rewrites them.
    fn name(&self) -> &'static str {
        "defradb-entities-v2"
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

//! Which indexes follow their fields through a pending log instead of inline.

use schema::{CollectionVersion, IndexDescription};

use crate::Crdts;

/// Whether `index` is deferred: an ordered, non-unique index over a field
/// whose writes are blind. A blind write never learns the value it lands on,
/// so it cannot move the entry; it owes the index an update instead. A unique
/// index stays inline, since its key has one winner.
pub fn deferred_index(
    index: &IndexDescription,
    collection: &CollectionVersion,
    crdts: &Crdts,
) -> bool {
    !index.is_vector()
        && !index.resolved_unique()
        && index.fields.iter().any(|indexed| {
            collection
                .field_by_name(&indexed.name)
                .and_then(|field| crdts.field(field.crdt_type))
                .is_some_and(|crdt| crdt.blind_op())
        })
}

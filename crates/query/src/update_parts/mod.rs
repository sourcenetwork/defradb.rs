//! Which parts of a document an update decides on.
//!
//! An update reads its document, decides, and writes some of its fields. At
//! commit its reads are validated only over the parts its decisions used, so
//! two updates of disjoint fields of one document never conflict. An update
//! decides on:
//! - each field it writes, except a counter increment, which is a blind
//!   operand and decides nothing (two writes of one field have one winner),
//!   and except, on a run after the write lost that race ([`Run`]), a field
//!   it only writes: the race was settled once, and the run merges its write
//!   on top of the winner;
//! - every component of an index over a field it writes, since the old index
//!   entry is found from them;
//! - every source of an embedding over a field it writes, and the embedding
//!   field itself;
//! - every field its filter compares.
//!
//! What an update returns is read, not decided on, so the fields it returns
//! add nothing. A schema migration needs no special case: a document whose
//! stored version is not current is read and validated whole by the projected
//! read itself, and is written back whole.
//!
//! For example `update_Users(filter: {name: {_eq: "a"}}, input: {age: 2})
//! { email }` on a collection indexed by `age` decides on `name` and `age`, so
//! a concurrent update of `email` commits beside it. What must never happen
//! is a decision on a part left out of the set: a concurrent change of that
//! part would commit unseen.
//!
//! [`planned`] answers for an update the planner is about to run, or says its
//! shape needs the whole document; [`written`] answers for a write applying a
//! set of modified fields. Both answer for a first run; [`Run::planned`] and
//! [`Run::written`] for the run an attempt is. Every update path, the planner,
//! the autocommit attempt and the explicit transaction, asks here, so they
//! cannot disagree.

use rapidhash::RapidHashSet;
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use storage::corekv::TransactionConflict;

use crate::mapper::Filter;

#[cfg(test)]
mod tests;

/// Which run of a write is deciding.
///
/// Of two concurrent writes of one field the first to commit wins, and the
/// other loses a race on that part and runs again (`Conflict` with the reason
/// [`regolith::Access::ReadParts`]). The run after the loss reads the
/// document's heads in its own transaction, so its write of the field sits
/// one priority above the winner and every write it could have seen. A field
/// it only writes then decides nothing: the run merges its write as an
/// operand, and a write it could not see is a concurrent one, settled by the
/// field's last-writer-wins order exactly as every replica settles the two
/// blocks. So a write loses a race on its fields at most once. A field whose
/// old value a rule reads stays decided on every run: an index component, an
/// embedding source, an immutable field, a field a filter compares, and every
/// field of a downsampled collection, whose series claim moves with the old
/// value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Run {
    /// The write's first run, or a run after it lost for another reason.
    #[default]
    First,
    /// A run after the write lost a race on a part it decided on.
    AfterLostPart,
}

impl Run {
    /// The run of an attempt whose previous attempt lost to `lost`, or of a
    /// first attempt.
    pub fn after(lost: Option<&TransactionConflict>) -> Self {
        match lost {
            Some(conflict) if conflict.access == regolith::Access::ReadParts => Self::AfterLostPart,
            _ => Self::First,
        }
    }

    /// [`planned`] for this run.
    pub fn planned(
        self,
        collection: &CollectionVersion,
        patch: &RapidHashSet<String>,
        filter: Option<&Filter>,
    ) -> Option<RapidHashSet<String>> {
        if collection.query.is_some() || patch.is_empty() {
            return None;
        }
        // Relation, object, and other structural inputs keep the whole read
        // until their nested mutation dependencies are enumerated here.
        let scalar = |name: &String| {
            field(collection, name)
                .is_some_and(|field| field.kind.is_scalar() && field.relation_name.is_none())
        };
        if !patch.iter().all(scalar) {
            return None;
        }
        let mut parts = self.written(collection, &collection.indexes, patch, |name| {
            field(collection, name)
                .and_then(|field| crdt::field_crdt(field.crdt_type))
                .is_some_and(|crdt| crdt.replicates_ops())
        });
        if let Some(filter) = filter {
            parts.extend(compared(collection, filter)?);
        }
        Some(parts)
    }

    /// [`written`] for this run.
    pub fn written<'a, I, F>(
        self,
        collection: &CollectionVersion,
        indexes: I,
        modified: &RapidHashSet<String>,
        is_increment: F,
    ) -> RapidHashSet<String>
    where
        I: IntoIterator<Item = &'a IndexDescription>,
        F: Fn(&str) -> bool,
    {
        let merged = |name: &str| {
            self == Self::AfterLostPart
                && collection.downsample_source.is_none()
                && field(collection, name).is_some_and(|field| !field.immutable)
        };
        let mut parts: RapidHashSet<String> = modified
            .iter()
            .filter(|name| !is_increment(name.as_str()) && !merged(name.as_str()))
            .cloned()
            .collect();
        for index in indexes {
            if index
                .fields
                .iter()
                .any(|field| modified.contains(&field.name))
            {
                parts.extend(index.fields.iter().map(|field| field.name.clone()));
            }
        }
        for embedding in &collection.vector_embeddings {
            if embedding
                .fields
                .iter()
                .any(|source| modified.contains(source))
            {
                parts.extend(embedding.fields.iter().cloned());
                parts.insert(embedding.field_name.clone());
            }
        }
        parts
    }
}

/// The parts a planned update of `patch`, selected by `filter`, decides on
/// in its first run. `None` when its shape needs the whole document
/// validated: a view, an empty patch, a patch of a relation or of a
/// non-scalar field, or a filter that reaches past the document (through a
/// relation or an alias).
pub fn planned(
    collection: &CollectionVersion,
    patch: &RapidHashSet<String>,
    filter: Option<&Filter>,
) -> Option<RapidHashSet<String>> {
    Run::First.planned(collection, patch, filter)
}

/// The parts a write applying `modified` decides on in its first run, given
/// the indexes it maintains. `is_increment` says which modified fields are
/// counter increments.
pub fn written<'a, I, F>(
    collection: &CollectionVersion,
    indexes: I,
    modified: &RapidHashSet<String>,
    is_increment: F,
) -> RapidHashSet<String>
where
    I: IntoIterator<Item = &'a IndexDescription>,
    F: Fn(&str) -> bool,
{
    Run::First.written(collection, indexes, modified, is_increment)
}

/// The stored fields `filter` compares, or `None` when it reaches past the
/// document. The document ID is the document's identity, not one of its parts.
fn compared(collection: &CollectionVersion, filter: &Filter) -> Option<Vec<String>> {
    if filter.has_relation_filters() || filter.has_alias_filter() {
        return None;
    }
    filter
        .referenced_fields()
        .into_iter()
        .filter(|name| name.as_str() != "_docID")
        .map(|name| {
            field(collection, &name)
                .filter(|field| stored_part(field))
                .map(|_| name)
        })
        .collect()
}

/// Whether a field is held as a part of the document's own value.
fn stored_part(field: &FieldDescription) -> bool {
    matches!(field.kind, FieldKind::Scalar(_) | FieldKind::ScalarArray(_))
        && field.relation_name.is_none()
}

fn field<'a>(collection: &'a CollectionVersion, name: &str) -> Option<&'a FieldDescription> {
    collection.fields.iter().find(|field| field.name == name)
}

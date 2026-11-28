//! The core's relationship types, under the names this crate's callers use.

pub(crate) use db::DocRelationship as Relationship;
pub use db::RelationshipChange;
#[cfg(test)]
pub(crate) use db::OWED_SETTLE_DEADLINE;

pub(super) use db::settle_owed_changes;

//! CRDT (Conflict-free Replicated Data Types) for DefraDB fields.
//!
//! - [`field`]: one [`FieldCrdt`] per CRDT type, the single place that says
//!   what a write, a block and a merge do to a field of that type.
//! - [`stored`]: the local document value, whose parts each hold one field's
//!   CRDT state and merge through its type.

pub mod field;
pub mod stored;

pub use field::{
    delta_crdt, field_crdt, part_crdt, DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite,
    MergeResult,
};

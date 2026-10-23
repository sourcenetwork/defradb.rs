//! The operations a write can express on one field.

use crate::NormalValue;

/// What a write does to a field. Which operations a field accepts is up to the
/// field's CRDT type.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldOp {
    /// Replace the field's value.
    Set(NormalValue),
    /// Add to the field's value.
    Increment(NormalValue),
}

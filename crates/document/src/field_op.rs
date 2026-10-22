//! The operations a write can express on one field.

use defra_core::TextOp;

use crate::NormalValue;

/// What a write does to a field. Which operations a field accepts is up to the
/// field's CRDT type.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldOp {
    /// Replace the field's value.
    Set(NormalValue),
    /// Add to the field's value.
    Increment(NormalValue),
    /// Edit the field's characters, in order: the first edit counts its
    /// positions in the stored value, each later one in the value the edits
    /// before it left.
    Edit(Vec<TextOp>),
}

impl FieldOp {
    /// This operation followed by `next` in the same write: edits append in
    /// order, as each later edit already counts in the value the earlier ones
    /// left; any other pair is replaced by `next`.
    pub fn then(self, next: FieldOp) -> FieldOp {
        match (self, next) {
            (FieldOp::Edit(mut edits), FieldOp::Edit(more)) => {
                edits.extend(more);
                FieldOp::Edit(edits)
            }
            (_, next) => next,
        }
    }

    /// The operation's name, as a refusal names it.
    pub fn name(&self) -> &'static str {
        match self {
            FieldOp::Set(_) => "set",
            FieldOp::Increment(_) => "increment",
            FieldOp::Edit(_) => "edit",
        }
    }
}

//! The operations a write can express on one field.

use defra_core::json_crdt::JsonOp;
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
    /// Edit the field's value at paths, in order. A set of an object sets each
    /// of its leaves at its own path and leaves every other key alone.
    Paths(Vec<JsonOp>),
    /// A write already prepared against the field's stored state, as a tagged
    /// type prepares it: the data its block carries, and the operand it lands
    /// on that state, encoded as a one-part document value.
    Operand { data: Vec<u8>, operand: Vec<u8> },
}

impl FieldOp {
    /// This operation followed by `next` in the same write: character edits
    /// and path edits append in order, as each later edit already acts on the
    /// value the earlier ones left; any other pair is replaced by `next`.
    pub fn then(self, next: FieldOp) -> FieldOp {
        match (self, next) {
            (FieldOp::Edit(mut edits), FieldOp::Edit(more)) => {
                edits.extend(more);
                FieldOp::Edit(edits)
            }
            (FieldOp::Paths(mut edits), FieldOp::Paths(more)) => {
                edits.extend(more);
                FieldOp::Paths(edits)
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
            FieldOp::Paths(_) => "path edits",
            FieldOp::Operand { .. } => "prepared operands",
        }
    }
}

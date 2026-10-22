//! The write side of a field CRDT type.
//!
//! One [`FieldCrdt`] per CRDT type answers everything the write path needs to
//! know about a field: what an operation does to its value, what its block
//! carries, how its document value is stored. The write path finds the type
//! through [`field_crdt`] and never branches on the kind.

mod counter;
mod lww;
pub mod testing;

use defra_core::block::CrdtDelta;
use document::stored::StoredField;
use document::{FieldOp, NormalValue};
use schema::{CType, FieldDescription};

pub use counter::{counter_kind, CounterField};
pub use lww::LwwField;

#[derive(Debug, thiserror::Error)]
pub enum FieldCrdtError {
    #[error("value cannot be negative")]
    NegativeIncrement,
    #[error("Counter fields only support numeric increments, got: {0}")]
    NonNumericIncrement(String),
    #[error("{crdt} fields do not support {op}")]
    UnsupportedOp { crdt: CType, op: &'static str },
    #[error("unsupported counter field {0}")]
    UnsupportedKind(String),
    #[error("{0}")]
    Nonce(String),
    #[error(transparent)]
    Stored(#[from] document::Error),
}

/// What a field block carries besides the data.
pub struct DeltaParts<'a> {
    pub field_name: &'a str,
    pub priority: u64,
    pub collection_version_id: &'a str,
    pub nonce: i64,
    /// The encoded (and possibly encrypted) value or operation.
    pub data: Vec<u8>,
}

/// A CRDT type's write behavior. Object safe, so a type is looked up by its
/// [`CType`] and used through `&dyn FieldCrdt`.
pub trait FieldCrdt: Send + Sync {
    fn ctype(&self) -> CType;

    /// The operation a user's input on this type expresses.
    fn input_op(&self, input: NormalValue) -> FieldOp;

    /// Reject an operation the type cannot take. `creating` is true when the
    /// operation is the field's first write.
    fn check(&self, op: &FieldOp, creating: bool) -> Result<(), FieldCrdtError>;

    /// The value `current` takes once `op` is applied to it.
    fn apply(
        &self,
        field: &FieldDescription,
        current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError>;

    /// The operation that leaves any value as it is, when the type has one.
    fn identity(&self, field: &FieldDescription) -> Option<FieldOp>;

    /// Whether an input always counts as a change, even one that restates the
    /// stored value: the input is an operation, not a state.
    fn always_changes(&self) -> bool;

    /// What the field's block carries: the value written, or the pending
    /// operation when the type's blocks carry operations.
    fn block_data<'a>(&self, value: &'a NormalValue, op: Option<&'a FieldOp>) -> &'a NormalValue;

    /// The nonce of the field's block. `fresh` draws a new one; a type that
    /// asks for none gets 0.
    fn nonce(
        &self,
        creating: bool,
        fresh: &mut dyn FnMut() -> Result<i64, FieldCrdtError>,
    ) -> Result<i64, FieldCrdtError>;

    /// The block delta in the Go-readable wire shape.
    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta;

    /// The field as the document value stores it.
    fn stored_field(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError>;

    /// What a local write merges into the document value, or `None` when the
    /// write has no operand form and the document value is put whole.
    fn operand<'a>(
        &self,
        value: &'a NormalValue,
        op: Option<&'a FieldOp>,
    ) -> Option<&'a NormalValue>;

    /// Whether the document value is built from operands: a nil value is not
    /// stored, and a resolved document never restates it.
    fn stored_by_operands(&self) -> bool;
}

static LWW: LwwField = LwwField;
static P_COUNTER: CounterField = CounterField::new(CType::PCounter);
static PN_COUNTER: CounterField = CounterField::new(CType::PnCounter);

/// The write behavior of a CRDT type, or `None` for a type with no field
/// blocks (`None`, `Object`, `Composite`, unknown).
pub fn field_crdt(ctype: CType) -> Option<&'static dyn FieldCrdt> {
    match ctype {
        CType::LwwRegister => Some(&LWW),
        CType::PCounter => Some(&P_COUNTER),
        CType::PnCounter => Some(&PN_COUNTER),
        _ => None,
    }
}

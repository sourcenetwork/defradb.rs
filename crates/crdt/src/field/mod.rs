//! Field CRDT types.
//!
//! One [`FieldCrdt`] per CRDT type answers everything a local write, a merge and
//! the document value need to know about a field: what an operation does to
//! its value, what its block carries, how its state is stored and how two
//! states merge. Callers find a type through [`field_crdt`] (by the field's
//! declared type), [`delta_crdt`] (by the block that carries it) or
//! [`part_crdt`] (by the stored part), and never branch on the kind.
//!
//! Most types merge a block into the stored value on its own. A type whose
//! operations count in the value they edit, such as positions in a text,
//! merges by causal replay instead ([`CausalCrdt`]).

mod causal;
mod counter;
mod lww;
pub mod testing;
mod text;

use defra_core::block::CrdtDelta;
use document::{FieldOp, NormalValue};
use schema::{CType, FieldDescription};

use crate::stored::{Part, StoredField, ValueKind};

pub use causal::{history_op, CausalBlock, CausalCrdt};
pub use counter::{counter_kind, CounterField};
pub use lww::LwwField;
pub use text::TextField;

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
    #[error("cannot decode the data of field {field}: {message}")]
    Decode { field: String, message: String },
    #[error("cannot encode block data: {0}")]
    Encode(String),
    #[error("the history of field {field} cannot be replayed: {message}")]
    History { field: String, message: String },
    #[error(transparent)]
    Stored(#[from] document::Error),
    #[error("field '{field}' is declared {declared}, but its block is of type {block}")]
    MismatchedBlock {
        field: String,
        declared: CType,
        block: &'static str,
    },
    #[error("not a field block")]
    NotAFieldBlock,
}

/// What a field block carries besides the data.
pub struct DeltaParts<'name> {
    pub field_name: &'name str,
    pub priority: u64,
    pub schema_version_id: &'name str,
    pub nonce: i64,
    /// The encoded (and possibly encrypted) value or operation.
    pub data: Vec<u8>,
}

/// What a local write's block carries, and how it reaches the document value.
pub struct FieldWrite<'value> {
    /// The value or operation the block carries, unless the type writes its
    /// block data otherwise ([`FieldCrdt::block_data`]), and the operand the
    /// document value takes.
    pub data: &'value NormalValue,
    /// Whether `data` merges into the stored value as an operand. When it does
    /// not, the document value is put whole.
    pub merges: bool,
}

/// What merging one state into another did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MergeResult {
    /// The incoming state changed the merged state.
    Applied,
    /// The incoming state lost to a higher priority.
    RejectedLowerPriority { current: u64, incoming: u64 },
    /// The incoming state lost the tie-break at an equal priority.
    RejectedTieBreak,
}

impl MergeResult {
    pub fn was_applied(&self) -> bool {
        matches!(self, MergeResult::Applied)
    }
}

/// A CRDT type. Object safe, so a type is looked up and used through
/// `&dyn FieldCrdt`.
pub trait FieldCrdt: Send + Sync {
    fn ctype(&self) -> CType;

    /// Whether the type replicates operations rather than states. An operation
    /// is never restated and must take effect exactly once: an input always
    /// counts as a change, an update's block carries a fresh nonce so equal
    /// operations stay distinct blocks, a nil value is not stored, and a merge
    /// applies each block once.
    fn replicates_ops(&self) -> bool;

    /// The operation an input expresses, refused when the type cannot take
    /// it. `creating` is true when the operation is the field's first write.
    fn op(&self, input: NormalValue, creating: bool) -> Result<FieldOp, FieldCrdtError>;

    /// The value `current` takes once `op` is applied to it.
    fn apply(
        &self,
        field: &FieldDescription,
        current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError>;

    /// What a local write of `value`, by `op` when it has one, puts in its
    /// block and merges into the document value.
    fn write<'value>(
        &self,
        value: &'value NormalValue,
        op: Option<&'value FieldOp>,
    ) -> FieldWrite<'value>;

    /// The data of the block a local write of `value`, by `op` when it has
    /// one, carries; `None` when the write leaves nothing to replicate.
    fn block_data(
        &self,
        value: &NormalValue,
        op: Option<&FieldOp>,
    ) -> Result<Option<Vec<u8>>, FieldCrdtError> {
        self.encode(self.write(value, op).data).map(Some)
    }

    /// The block data of `data`, the value or operation a block carries.
    fn encode(&self, data: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
        let mut bytes = Vec::with_capacity(data.cbor_size_hint());
        ciborium::into_writer(data, &mut bytes)
            .map_err(|error| FieldCrdtError::Encode(error.to_string()))?;
        Ok(bytes)
    }

    /// The value or operation a block's data carries, read as `field` declares.
    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError>;

    /// The block delta in the Go-readable wire shape.
    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta;

    /// The state of `field` holding `value` at `priority`, as the document
    /// value stores it.
    fn state(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError>;

    /// The state a block at `priority` carrying `data` merges in.
    fn block_state(
        &self,
        field: &FieldDescription,
        priority: u64,
        data: &[u8],
    ) -> Result<StoredField, FieldCrdtError> {
        self.state(field, priority, &self.decode(field, data)?)
    }

    /// The value a stored state reads as, or `None` when its bytes are not a
    /// state of this type.
    fn materialize(&self, state: Part<'_>) -> Option<NormalValue>;

    /// Merge `incoming` into `state`, both of this type. A state with empty
    /// bytes is the type's starting state. `None` when either is malformed.
    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult>;

    /// Whether merges into states like `state` give the same result in any
    /// grouping, so two pending operands may fold before their base is known.
    fn folds_exactly(&self, state: Part<'_>) -> bool;

    /// How the type merges by causal replay, for a type whose operations
    /// count in the value they edit; `None` for a type whose blocks merge into
    /// the stored value one by one.
    fn causal(&self) -> Option<&dyn CausalCrdt> {
        None
    }

    /// Whether a write's operation is blind: it does not depend on the value
    /// it lands on, so the write decides nothing on that value. True for a
    /// type that replicates operations, unless they count in the value they
    /// edit.
    fn blind_op(&self) -> bool {
        self.replicates_ops() && self.causal().is_none()
    }
}

static LWW: LwwField = LwwField;
static P_COUNTER: CounterField = CounterField::new(CType::PCounter);
static PN_COUNTER: CounterField = CounterField::new(CType::PnCounter);
static TEXT: TextField = TextField;

/// The CRDT type a field declares, or `None` for a type with no field blocks
/// (`None`, `Object`, `Composite`, unknown).
pub fn field_crdt(ctype: CType) -> Option<&'static dyn FieldCrdt> {
    match ctype {
        CType::LwwRegister => Some(&LWW),
        CType::PCounter => Some(&P_COUNTER),
        CType::PnCounter => Some(&PN_COUNTER),
        CType::Text => Some(&TEXT),
        _ => None,
    }
}

/// The CRDT type that wrote a field block, by its wire variant; a counter
/// block takes the field's declared counter, positive unless it allows
/// decrements. A block of another family than the declared type is refused,
/// as Go refuses it: merging it would sum into a register or overwrite a
/// counter. `CType::None` declares nothing, so any field block is read as its
/// variant says.
pub fn delta_crdt(
    delta: &CrdtDelta,
    declared: CType,
) -> Result<&'static dyn FieldCrdt, FieldCrdtError> {
    let mismatched = |block: &'static str| FieldCrdtError::MismatchedBlock {
        field: delta
            .field()
            .map(|field| field.field_name.to_owned())
            .unwrap_or_default(),
        declared,
        block,
    };
    match (delta, declared) {
        (CrdtDelta::Text(_), CType::Text | CType::None) => Ok(&TEXT),
        (CrdtDelta::Text(_), _) => Err(mismatched("text")),
        (CrdtDelta::Lww(_), CType::PCounter | CType::PnCounter | CType::Text) => {
            Err(mismatched("LWW"))
        }
        (CrdtDelta::Lww(_), _) => Ok(&LWW),
        (CrdtDelta::Counter(_), CType::LwwRegister | CType::Text) => Err(mismatched("counter")),
        (CrdtDelta::Counter(_), CType::PnCounter) => Ok(&PN_COUNTER),
        (CrdtDelta::Counter(_), _) => Ok(&P_COUNTER),
        _ => Err(FieldCrdtError::NotAFieldBlock),
    }
}

/// The CRDT type that stores and merges a part of `kind`. Both counters store
/// and merge alike.
pub fn part_crdt(kind: ValueKind) -> &'static dyn FieldCrdt {
    match kind {
        ValueKind::Lww => &LWW,
        ValueKind::Int64 | ValueKind::Float32 | ValueKind::Float64 => &PN_COUNTER,
        ValueKind::Text => &TEXT,
    }
}

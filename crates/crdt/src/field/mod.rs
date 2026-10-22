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
//! merges by causal replay instead ([`CausalCrdt`]). A type whose state
//! records its operations by ID, and whose operations act only on those their
//! writer had seen, merges by its block's causal past ([`TaggedCrdt`]).
//!
//! The trait is also the contract for a custom type, one a third party
//! implements and registers on the node builder under a [`CType::Custom`]
//! name ([`register`]). Its laws are stated method by method below, and
//! [`testing::check_field_crdt`] checks them.

mod causal;
mod counter;
mod json;
mod lww;
mod registry;
pub mod tagged;
pub mod testing;
mod text;
mod unregistered;

use defra_core::block::CrdtDelta;
use document::{FieldOp, NormalValue};
use schema::{CType, CrdtName, FieldDescription};

use crate::stored::{Part, StoredField, ValueKind};

pub use causal::{history_op, CausalBlock, CausalCrdt};
pub use counter::{counter_kind, CounterField};
pub use json::JsonField;
pub use lww::LwwField;
pub use registry::{CrdtRef, Crdts, PartCrdts, Registration, RegistryError};
pub use tagged::{InPast, Tag, TaggedBlock, TaggedCrdt};
pub use text::TextField;

#[derive(Debug, thiserror::Error)]
pub enum FieldCrdtError {
    #[error("value cannot be negative")]
    NegativeIncrement,
    #[error("Counter fields only support numeric increments, got: {0}")]
    NonNumericIncrement(String),
    #[error("{crdt} fields do not support {op}")]
    UnsupportedOp { crdt: CType, op: &'static str },
    #[error("{crdt} fields merge increments and cannot be set; use `increment`")]
    SetOnCounter { crdt: CType },
    #[error("{crdt} fields cannot hold {message}")]
    InvalidInput { crdt: CType, message: String },
    #[error("unsupported counter field {0}")]
    UnsupportedKind(String),
    #[error("cannot decode the data of field {field}: {message}")]
    Decode { field: String, message: String },
    #[error("cannot encode block data: {0}")]
    Encode(String),
    #[error("a block of field {field} contradicts its own version: {message}")]
    Version { field: String, message: String },
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
    #[error(
        "'{crdt}' is a custom CRDT type this node does not have; register it with \
         NodeBuilder::with_crdt to write or merge its fields"
    )]
    Unregistered { crdt: CrdtName },
}

/// What a field block carries besides the data.
pub struct DeltaParts<'name> {
    pub field_name: &'name str,
    pub priority: u64,
    pub collection_version_id: &'name str,
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
///
/// Every method is a pure function of its arguments: replicas call them in
/// different orders and on different threads, and must reach the same state.
/// [`testing::check_field_crdt`] checks the laws below on sample values.
pub trait FieldCrdt: Send + Sync {
    /// The type's name in field definitions. Constant for the type; a custom
    /// type answers `CType::Custom` of the name it registers under.
    fn ctype(&self) -> CType;

    /// Whether the type replicates operations rather than states. An operation
    /// is never restated and must take effect exactly once: an input always
    /// counts as a change, an update's block carries a fresh nonce so equal
    /// operations stay distinct blocks, a nil value is not stored, and a merge
    /// applies each block once. A state type's merge is idempotent instead.
    fn replicates_ops(&self) -> bool;

    /// The operation an input expresses, refused when the type cannot take
    /// it. `creating` is true when the operation is the field's first write.
    /// Law: an operation this accepts as a first write applies to every value
    /// the type stores.
    fn op(&self, input: NormalValue, creating: bool) -> Result<FieldOp, FieldCrdtError>;

    /// The operation a client names, refused when the type cannot take it.
    /// A named set restates the value; by default it is the only one taken.
    fn named_op(&self, op: FieldOp) -> Result<FieldOp, FieldCrdtError> {
        match op {
            FieldOp::Set(value) => Ok(FieldOp::Set(value)),
            op => Err(FieldCrdtError::UnsupportedOp {
                crdt: self.ctype(),
                op: op.name(),
            }),
        }
    }

    /// The value `current` takes once `op` is applied to it. Law: the
    /// value equals what merging the write's state ([`write`](Self::write),
    /// [`state`](Self::state)) into `current`'s state reads as.
    fn apply(
        &self,
        field: &FieldDescription,
        current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError>;

    /// What a local write of `value`, by `op` when it has one, puts in its
    /// block and merges into the document value. `value` is the field's value
    /// after [`apply`](Self::apply). A state type's write merges.
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
    /// Law: [`decode`](Self::decode) reads it back as `data`. Block data is
    /// content-addressed and replicated, so the encoding never changes once
    /// a block of the type exists.
    fn encode(&self, data: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
        let mut bytes = Vec::with_capacity(data.cbor_size_hint());
        ciborium::into_writer(data, &mut bytes)
            .map_err(|error| FieldCrdtError::Encode(error.to_string()))?;
        Ok(bytes)
    }

    /// The value or operation a block's data carries, read as `field` declares.
    /// Refuses bytes that are not the type's data, never panics on them: they
    /// come from peers.
    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError>;

    /// The block delta in the Go-readable wire shape. Law: it carries the
    /// field name, priority, version ID and data it is given, and a blind
    /// operation's delta carries the nonce. A custom type writes
    /// [`CrdtDelta::Custom`] naming itself.
    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta;

    /// The state of `field` holding `value` at `priority`, as the document
    /// value stores it. Law: [`materialize`](Self::materialize) reads it back
    /// as `value`. A custom type's state is of kind
    /// [`ValueKind::Custom`](crate::stored::ValueKind::Custom) of its name, so
    /// the store's merge operator finds the type from the part alone.
    fn state(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError>;

    /// The state a block at `priority` carrying `data` merges in. Law: it
    /// reads as the state of the value `data` decodes to.
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
    /// Laws: merges commute; a state type's merge is idempotent and picks the
    /// same winner in any order; where [`folds_exactly`](Self::folds_exactly)
    /// holds, merges associate. The store runs it inside regolith's merge
    /// operator, so it neither blocks nor reaches outside its arguments.
    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult>;

    /// Whether merges into states like `state` give the same result in any
    /// grouping, so two pending operands may fold before their base is known.
    fn folds_exactly(&self, state: Part<'_>) -> bool;

    /// How the type merges by causal replay, for a type whose operations
    /// count in the value they edit; `None` for a type whose blocks merge into
    /// the stored value one by one. Laws: [`testing::check_causal`].
    fn causal(&self) -> Option<&dyn CausalCrdt> {
        None
    }

    /// How the type merges by its blocks' causal past, for a type whose state
    /// records its operations by ID; `None` for any other. A type is at most
    /// one of causal and tagged. Laws: [`testing::check_tagged`].
    fn tagged(&self) -> Option<&dyn TaggedCrdt> {
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
static JSON: JsonField = JsonField;

/// The native CRDT type a field declares, or `None` for a type with no field
/// blocks (`None`, `Object`, `Composite`, unknown) and for a custom type,
/// which only a node's [`Crdts`] knows: a field of a stored definition is
/// looked up through [`Crdts::field`].
pub fn native(ctype: CType) -> Option<&'static dyn FieldCrdt> {
    match ctype {
        CType::LwwRegister => Some(&LWW),
        CType::PCounter => Some(&P_COUNTER),
        CType::PnCounter => Some(&PN_COUNTER),
        CType::Text => Some(&TEXT),
        CType::Json => Some(&JSON),
        _ => None,
    }
}

/// The native CRDT type of a stored part's kind, or `None` for a custom
/// kind, which only a node's [`Crdts`] knows.
pub(crate) fn native_part(kind: ValueKind) -> Option<&'static dyn FieldCrdt> {
    match kind {
        ValueKind::Lww => Some(&LWW),
        ValueKind::Int64 | ValueKind::Float32 | ValueKind::Float64 => Some(&PN_COUNTER),
        ValueKind::Text => Some(&TEXT),
        ValueKind::Json => Some(&JSON),
        ValueKind::Custom(_) => None,
    }
}

/// The native CRDT type that wrote a field block ([`Crdts::delta`]), or the
/// name of the custom type a custom block names, once it is checked against
/// the declared type.
pub(crate) fn native_delta(
    delta: &CrdtDelta,
    declared: CType,
) -> Result<Result<&'static dyn FieldCrdt, CrdtName>, FieldCrdtError> {
    let mismatched = |block: &'static str| FieldCrdtError::MismatchedBlock {
        field: delta
            .field()
            .map(|field| field.field_name.to_owned())
            .unwrap_or_default(),
        declared,
        block,
    };
    let native: &'static dyn FieldCrdt = match (delta, declared) {
        (CrdtDelta::Custom(custom), CType::Custom(_) | CType::None) => {
            let named = CrdtName::new(&custom.crdt).map_err(|_| mismatched("custom"))?;
            if declared != CType::None && declared != CType::Custom(named) {
                return Err(mismatched("custom"));
            }
            return Ok(Err(named));
        }
        (CrdtDelta::Custom(_), _) => return Err(mismatched("custom")),
        (_, CType::Custom(_)) if delta.field().is_some() => {
            return Err(mismatched(native_block(delta)))
        }
        (CrdtDelta::Text(_), CType::Text | CType::None) => &TEXT,
        (CrdtDelta::Text(_), _) => return Err(mismatched("text")),
        (CrdtDelta::Json(_), CType::Json | CType::None) => &JSON,
        (CrdtDelta::Json(_), _) => return Err(mismatched("json")),
        (CrdtDelta::Lww(_), CType::PCounter | CType::PnCounter | CType::Text | CType::Json) => {
            return Err(mismatched("LWW"))
        }
        (CrdtDelta::Lww(_), _) => &LWW,
        (CrdtDelta::Counter(_), CType::LwwRegister | CType::Text | CType::Json) => {
            return Err(mismatched("counter"))
        }
        (CrdtDelta::Counter(_), CType::PnCounter) => &PN_COUNTER,
        (CrdtDelta::Counter(_), _) => &P_COUNTER,
        _ => return Err(FieldCrdtError::NotAFieldBlock),
    };
    Ok(Ok(native))
}

/// The name of a native field block's family, as a refusal names it.
fn native_block(delta: &CrdtDelta) -> &'static str {
    match delta {
        CrdtDelta::Lww(_) => "LWW",
        CrdtDelta::Counter(_) => "counter",
        CrdtDelta::Text(_) => "text",
        CrdtDelta::Json(_) => "json",
        _ => "custom",
    }
}

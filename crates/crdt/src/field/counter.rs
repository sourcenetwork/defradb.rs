use defra_core::block::CrdtDelta;
use defra_core::block_delta::CounterDeltaPayload;
use document::{FieldOp, NormalValue};
use schema::{CType, FieldDescription, FieldKind, ScalarKind};

use super::{DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite, MergeResult};
use crate::stored::{Part, StoredField, ValueKind};

/// How the document value stores a counter field, or `None` when the field is
/// not a counter of a numeric kind.
pub fn counter_kind(field: &FieldDescription) -> Option<ValueKind> {
    if !field.crdt_type.is_counter() {
        return None;
    }
    counter_value_kind(&field.kind)
}

fn counter_value_kind(kind: &FieldKind) -> Option<ValueKind> {
    match kind.as_scalar().map(ScalarKind::base_kind) {
        Some(ScalarKind::NillableInt) => Some(ValueKind::Int64),
        Some(ScalarKind::NillableFloat32) => Some(ValueKind::Float32),
        Some(ScalarKind::NillableFloat64) => Some(ValueKind::Float64),
        _ => None,
    }
}

fn kind_of(field: &FieldDescription) -> Result<ValueKind, FieldCrdtError> {
    counter_value_kind(&field.kind)
        .ok_or_else(|| FieldCrdtError::UnsupportedKind(field.name.clone()))
}

fn is_negative(value: &NormalValue) -> bool {
    match value {
        NormalValue::Int(v) => *v < 0,
        NormalValue::Float64(v) => *v < 0.0,
        NormalValue::Float32(v) => *v < 0.0,
        _ => false,
    }
}

/// `current + increment`, both big-endian numbers of `kind`: wrapping for
/// integers, IEEE-754 addition at the kind's own precision for floats, as Go's
/// `validateAndIncrement` adds them. Empty `current` bytes count as zero.
fn sum(kind: ValueKind, current: &[u8], increment: &[u8]) -> Option<Vec<u8>> {
    const ZERO: [u8; 8] = [0; 8];
    let width = if kind == ValueKind::Float32 { 4 } else { 8 };
    let current = if current.is_empty() {
        &ZERO[..width]
    } else {
        current
    };
    Some(match kind {
        ValueKind::Int64 => i64::from_be_bytes(current.try_into().ok()?)
            .wrapping_add(i64::from_be_bytes(increment.try_into().ok()?))
            .to_be_bytes()
            .to_vec(),
        ValueKind::Float32 => (f32::from_be_bytes(current.try_into().ok()?)
            + f32::from_be_bytes(increment.try_into().ok()?))
        .to_be_bytes()
        .to_vec(),
        ValueKind::Float64 => (f64::from_be_bytes(current.try_into().ok()?)
            + f64::from_be_bytes(increment.try_into().ok()?))
        .to_be_bytes()
        .to_vec(),
        ValueKind::Lww | ValueKind::Text | ValueKind::Json => return None,
    })
}

/// Counters: a write is an increment, blocks carry increments, and a state is
/// the sum of every increment merged into it. The positive counter refuses
/// negative values, on create and on update.
///
/// Float sums are not associative (`(0.1 + 0.2) + 0.3 != 0.1 + (0.2 + 0.3)`,
/// proven as `float_add_not_assoc`), so replicas that merge the same float
/// increments in different orders can hold different values. Go adds them the
/// same way, so this is kept for parity; integer counters converge.
pub struct CounterField {
    ctype: CType,
}

impl CounterField {
    pub const fn new(ctype: CType) -> Self {
        Self { ctype }
    }

    /// The stored state of a counter named `name` of `kind` holding `value`:
    /// its value for a document put, or its increment for an operand.
    /// Integers, `Float32` and `Float64` convert as the kind says.
    pub fn stored(
        name: String,
        kind: ValueKind,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        let bytes = match kind {
            ValueKind::Int64 => value.as_int().map(|v| v.to_be_bytes().to_vec()),
            ValueKind::Float32 => value
                .as_float32()
                .or_else(|| value.as_float64().map(|v| v as f32))
                .or_else(|| value.as_int().map(|v| v as f32))
                .map(|v| v.to_be_bytes().to_vec()),
            ValueKind::Float64 => value
                .as_float64()
                .or_else(|| value.as_float32().map(f64::from))
                .or_else(|| value.as_int().map(|v| v as f64))
                .map(|v| v.to_be_bytes().to_vec()),
            ValueKind::Lww | ValueKind::Text | ValueKind::Json => None,
        };
        let Some(value) = bytes else {
            return Err(document::Error::InvalidFieldValue {
                field: name,
                message: format!("not a counter value of kind {kind:?}"),
            }
            .into());
        };
        Ok(StoredField {
            name,
            kind,
            priority,
            value,
        })
    }
}

impl FieldCrdt for CounterField {
    fn ctype(&self) -> CType {
        self.ctype
    }

    fn replicates_ops(&self) -> bool {
        true
    }

    fn op(&self, input: NormalValue, _creating: bool) -> Result<FieldOp, FieldCrdtError> {
        if !self.ctype.allows_decrement() && is_negative(&input) {
            return Err(FieldCrdtError::NegativeIncrement);
        }
        Ok(FieldOp::Increment(input))
    }

    /// Two peers setting a counter at once would each add their own gap, so
    /// a client names increments only.
    fn named_op(&self, op: FieldOp) -> Result<FieldOp, FieldCrdtError> {
        match op {
            FieldOp::Increment(by) => self.op(by, false),
            FieldOp::Set(_) => Err(FieldCrdtError::SetOnCounter { crdt: self.ctype }),
            op => Err(FieldCrdtError::UnsupportedOp {
                crdt: self.ctype,
                op: op.name(),
            }),
        }
    }

    fn apply(
        &self,
        field: &FieldDescription,
        current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError> {
        let by = match op {
            FieldOp::Set(value) => return Ok(value.clone()),
            FieldOp::Increment(by) => by,
            op => {
                return Err(FieldCrdtError::UnsupportedOp {
                    crdt: self.ctype,
                    op: op.name(),
                })
            }
        };
        if !matches!(
            by,
            NormalValue::Int(_) | NormalValue::Float32(_) | NormalValue::Float64(_)
        ) {
            return Err(FieldCrdtError::NonNumericIncrement(format!("{by:?}")));
        }
        let mut state = match current.filter(|value| !value.is_nil()) {
            Some(current) => self.state(field, 0, current)?,
            None => StoredField {
                name: field.name.clone(),
                kind: kind_of(field)?,
                priority: 0,
                value: Vec::new(),
            },
        };
        let sum = || document::Error::InvalidFieldValue {
            field: field.name.clone(),
            message: "the counter sum cannot be stored".into(),
        };
        self.merge(&mut state, self.state(field, 0, by)?)
            .ok_or_else(sum)?;
        Ok(self.materialize(state.part()).ok_or_else(sum)?)
    }

    fn write<'value>(
        &self,
        value: &'value NormalValue,
        op: Option<&'value FieldOp>,
    ) -> FieldWrite<'value> {
        match op {
            Some(FieldOp::Increment(by)) => FieldWrite {
                data: by,
                merges: true,
            },
            _ => FieldWrite {
                data: value,
                merges: false,
            },
        }
    }

    /// Go encodes an increment as the CBOR number of the field's kind.
    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError> {
        let decode = |error: ciborium::de::Error<std::io::Error>| FieldCrdtError::Decode {
            field: field.name.clone(),
            message: error.to_string(),
        };
        Ok(match kind_of(field)? {
            ValueKind::Int64 => NormalValue::Int(ciborium::from_reader(data).map_err(decode)?),
            ValueKind::Float32 => {
                NormalValue::Float32(ciborium::from_reader(data).map_err(decode)?)
            }
            ValueKind::Float64 => {
                NormalValue::Float64(ciborium::from_reader(data).map_err(decode)?)
            }
            ValueKind::Lww | ValueKind::Text | ValueKind::Json => {
                return Err(FieldCrdtError::UnsupportedKind(field.name.clone()))
            }
        })
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Counter(CounterDeltaPayload {
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
            nonce: parts.nonce,
            collection_version_id: parts.collection_version_id.to_owned(),
            data: parts.data,
        })
    }

    fn state(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        Self::stored(field.name.clone(), kind_of(field)?, priority, value)
    }

    /// A `Float32` counter reads as a `Float64` of its `f32` sum, as every
    /// stored float does.
    fn materialize(&self, state: Part<'_>) -> Option<NormalValue> {
        let value = state.value;
        match state.kind {
            ValueKind::Int64 => Some(NormalValue::Int(i64::from_be_bytes(value.try_into().ok()?))),
            ValueKind::Float32 => Some(NormalValue::Float64(f64::from(f32::from_be_bytes(
                value.try_into().ok()?,
            )))),
            ValueKind::Float64 => Some(NormalValue::Float64(f64::from_be_bytes(
                value.try_into().ok()?,
            ))),
            ValueKind::Lww | ValueKind::Text | ValueKind::Json => None,
        }
    }

    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult> {
        state.value = sum(state.kind, &state.value, &incoming.value)?;
        state.priority = state.priority.max(incoming.priority);
        Some(MergeResult::Applied)
    }

    /// Wrapping integer sums are exact in any grouping; float sums round, so
    /// they keep their order.
    fn folds_exactly(&self, state: Part<'_>) -> bool {
        state.kind == ValueKind::Int64
    }
}

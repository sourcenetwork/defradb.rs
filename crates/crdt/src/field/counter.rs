use defra_core::block::CrdtDelta;
use defra_core::block_delta::CounterDeltaPayload;
use document::stored::{add_counter, StoredField, ValueKind};
use document::{FieldOp, NormalValue};
use schema::{CType, FieldDescription, FieldKind, ScalarKind};

use super::{DeltaParts, FieldCrdt, FieldCrdtError};

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

fn is_negative(value: &NormalValue) -> bool {
    match value {
        NormalValue::Int(v) => *v < 0,
        NormalValue::Float64(v) => *v < 0.0,
        NormalValue::Float32(v) => *v < 0.0,
        _ => false,
    }
}

/// Counters: a write is an increment, and blocks carry increments. The
/// positive counter refuses negative increments on update.
pub struct CounterField {
    ctype: CType,
}

impl CounterField {
    pub const fn new(ctype: CType) -> Self {
        Self { ctype }
    }
}

impl FieldCrdt for CounterField {
    fn ctype(&self) -> CType {
        self.ctype
    }

    fn input_op(&self, input: NormalValue) -> FieldOp {
        FieldOp::Increment(input)
    }

    fn check(&self, op: &FieldOp, creating: bool) -> Result<(), FieldCrdtError> {
        match op {
            FieldOp::Increment(by) if !creating && !self.ctype.allows_decrement() => {
                if is_negative(by) {
                    return Err(FieldCrdtError::NegativeIncrement);
                }
                Ok(())
            }
            _ => Ok(()),
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
        };
        if !matches!(
            by,
            NormalValue::Int(_) | NormalValue::Float32(_) | NormalValue::Float64(_)
        ) {
            return Err(FieldCrdtError::NonNumericIncrement(format!("{by:?}")));
        }
        let kind = counter_value_kind(&field.kind)
            .ok_or_else(|| FieldCrdtError::UnsupportedKind(field.name.clone()))?;
        let current = current.filter(|value| !value.is_nil());
        Ok(add_counter(&field.name, kind, current, by)?)
    }

    fn identity(&self, field: &FieldDescription) -> Option<FieldOp> {
        let zero = match counter_value_kind(&field.kind)? {
            ValueKind::Int64 => NormalValue::Int(0),
            ValueKind::Float32 => NormalValue::Float32(0.0),
            ValueKind::Float64 => NormalValue::Float64(0.0),
            ValueKind::Lww => return None,
        };
        Some(FieldOp::Increment(zero))
    }

    fn always_changes(&self) -> bool {
        true
    }

    fn block_data<'a>(&self, value: &'a NormalValue, op: Option<&'a FieldOp>) -> &'a NormalValue {
        match op {
            Some(FieldOp::Increment(by)) => by,
            _ => value,
        }
    }

    fn nonce(
        &self,
        creating: bool,
        fresh: &mut dyn FnMut() -> Result<i64, FieldCrdtError>,
    ) -> Result<i64, FieldCrdtError> {
        if creating {
            Ok(0)
        } else {
            fresh()
        }
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

    fn stored_field(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        let kind = counter_value_kind(&field.kind)
            .ok_or_else(|| FieldCrdtError::UnsupportedKind(field.name.clone()))?;
        Ok(StoredField::counter(
            field.name.clone(),
            kind,
            priority,
            value,
        )?)
    }

    fn operand<'a>(
        &self,
        _value: &'a NormalValue,
        op: Option<&'a FieldOp>,
    ) -> Option<&'a NormalValue> {
        match op {
            Some(FieldOp::Increment(by)) => Some(by),
            _ => None,
        }
    }

    fn stored_by_operands(&self) -> bool {
        true
    }
}

use defra_core::block::CrdtDelta;
use defra_core::block_delta::LwwDeltaPayload;
use document::stored::StoredField;
use document::{FieldOp, NormalValue};
use schema::{CType, FieldDescription};

use super::{DeltaParts, FieldCrdt, FieldCrdtError};

/// Last-write-wins register: every write restates the whole value.
pub struct LwwField;

impl FieldCrdt for LwwField {
    fn ctype(&self) -> CType {
        CType::LwwRegister
    }

    fn input_op(&self, input: NormalValue) -> FieldOp {
        FieldOp::Set(input)
    }

    fn check(&self, op: &FieldOp, _creating: bool) -> Result<(), FieldCrdtError> {
        match op {
            FieldOp::Set(_) => Ok(()),
            FieldOp::Increment(_) => Err(FieldCrdtError::UnsupportedOp {
                crdt: CType::LwwRegister,
                op: "increment",
            }),
        }
    }

    fn apply(
        &self,
        _field: &FieldDescription,
        _current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError> {
        self.check(op, false)?;
        match op {
            FieldOp::Set(value) | FieldOp::Increment(value) => Ok(value.clone()),
        }
    }

    fn identity(&self, _field: &FieldDescription) -> Option<FieldOp> {
        None
    }

    fn always_changes(&self) -> bool {
        false
    }

    fn block_data<'a>(&self, value: &'a NormalValue, _op: Option<&'a FieldOp>) -> &'a NormalValue {
        value
    }

    fn nonce(
        &self,
        _creating: bool,
        _fresh: &mut dyn FnMut() -> Result<i64, FieldCrdtError>,
    ) -> Result<i64, FieldCrdtError> {
        Ok(0)
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
            schema_version_id: parts.schema_version_id.to_owned(),
            data: parts.data,
        })
    }

    fn stored_field(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        Ok(StoredField::lww(field.name.clone(), priority, value)?)
    }

    fn operand<'a>(
        &self,
        value: &'a NormalValue,
        _op: Option<&'a FieldOp>,
    ) -> Option<&'a NormalValue> {
        Some(value)
    }

    fn stored_by_operands(&self) -> bool {
        false
    }
}

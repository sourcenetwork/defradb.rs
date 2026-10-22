//! A max register, written as a third party would write a custom CRDT type:
//! an integer field whose value is the greatest any replica ever wrote.

use std::marker::PhantomData;

use crdt::stored::{Part, StoredField, ValueKind};
use crdt::{DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite, MergeResult};
use defra_core::{CrdtDelta, CustomDeltaPayload};
use document::{FieldOp, NormalValue};
use schema::{CType, CrdtName, FieldDescription};

pub const NAME: &str = "maxreg";

/// `Tag` makes another Rust type of the same register, and `ctype` lets one
/// claim another type's name, for the registry's refusals.
pub struct MaxRegister<Tag = ()> {
    ctype: CType,
    tag: PhantomData<fn() -> Tag>,
}

impl MaxRegister {
    pub fn new() -> Self {
        Self::claiming(CType::Custom(name()))
    }
}

impl<Tag> MaxRegister<Tag> {
    pub fn claiming(ctype: CType) -> Self {
        Self {
            ctype,
            tag: PhantomData,
        }
    }
}

fn name() -> CrdtName {
    CrdtName::new(NAME).unwrap()
}

/// A register holds nothing (nil) or an integer, as eight big-endian bytes.
fn bytes(value: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
    match value {
        NormalValue::Null => Ok(Vec::new()),
        NormalValue::Int(value) => Ok(value.to_be_bytes().to_vec()),
        other => Err(FieldCrdtError::InvalidInput {
            crdt: CType::Custom(name()),
            message: format!("{other:?}; a max register holds integers"),
        }),
    }
}

fn value(bytes: &[u8]) -> Option<NormalValue> {
    match bytes {
        [] => Some(NormalValue::Null),
        bytes => Some(NormalValue::Int(i64::from_be_bytes(bytes.try_into().ok()?))),
    }
}

/// Nil below every integer, so a register only ever grows.
fn rank(bytes: &[u8]) -> Option<i128> {
    match value(bytes)? {
        NormalValue::Int(value) => Some(i128::from(value)),
        _ => Some(i128::MIN),
    }
}

impl<Tag> FieldCrdt for MaxRegister<Tag>
where
    Tag: 'static,
{
    fn ctype(&self) -> CType {
        self.ctype
    }

    fn replicates_ops(&self) -> bool {
        false
    }

    fn op(&self, input: NormalValue, _creating: bool) -> Result<FieldOp, FieldCrdtError> {
        bytes(&input)?;
        Ok(FieldOp::Set(input))
    }

    fn apply(
        &self,
        _field: &FieldDescription,
        current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError> {
        let FieldOp::Set(input) = op else {
            return Err(FieldCrdtError::UnsupportedOp {
                crdt: self.ctype(),
                op: op.name(),
            });
        };
        let current = current.unwrap_or(&NormalValue::Null);
        Ok(match (rank(&bytes(current)?), rank(&bytes(input)?)) {
            (Some(current_rank), Some(input_rank)) if current_rank > input_rank => current.clone(),
            _ => input.clone(),
        })
    }

    fn write<'value>(
        &self,
        value: &'value NormalValue,
        _op: Option<&'value FieldOp>,
    ) -> FieldWrite<'value> {
        FieldWrite {
            data: value,
            merges: true,
        }
    }

    fn encode(&self, data: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
        bytes(data)
    }

    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError> {
        value(data).ok_or_else(|| FieldCrdtError::Decode {
            field: field.name.clone(),
            message: format!("{} bytes are not a max register", data.len()),
        })
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Custom(CustomDeltaPayload {
            crdt: NAME.to_owned(),
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
            nonce: None,
            schema_version_id: parts.schema_version_id.to_owned(),
            data: parts.data,
        })
    }

    fn state(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        Ok(StoredField {
            name: field.name.clone(),
            kind: ValueKind::Custom(name()),
            priority,
            value: bytes(value)?,
        })
    }

    fn materialize(&self, state: Part<'_>) -> Option<NormalValue> {
        (state.kind == ValueKind::Custom(name()))
            .then(|| value(state.value))
            .flatten()
    }

    /// The greater value wins, then the higher priority: a join on a total
    /// order, so merges commute, associate and repeat harmlessly.
    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult> {
        let current = (rank(&state.value)?, state.priority);
        let offered = (rank(&incoming.value)?, incoming.priority);
        if offered <= current {
            return Some(MergeResult::RejectedTieBreak);
        }
        state.priority = incoming.priority;
        state.value = incoming.value;
        Some(MergeResult::Applied)
    }

    fn folds_exactly(&self, _state: Part<'_>) -> bool {
        true
    }
}

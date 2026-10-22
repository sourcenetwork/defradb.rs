use defra_core::block::CrdtDelta;
use defra_core::block_delta::LwwDeltaPayload;
use document::encoding::{cbor_to_normal_value, normal_value_to_cbor};
use document::{FieldOp, NormalValue};
use schema::{CType, FieldDescription};

use super::{DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite, MergeResult};
use crate::stored::{Part, StoredField, ValueKind};

/// Last-write-wins register: every write restates the whole value, and of two
/// states the one with the higher priority wins, then the one with the greater
/// bytes. Empty bytes are a tombstone, ordered like any other value.
pub struct LwwField;

impl LwwField {
    /// The stored state of a register named `name` holding `value`: nothing
    /// for nil, its CBOR otherwise.
    pub fn stored(
        name: String,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        let value = if value.is_nil() {
            Vec::new()
        } else {
            let mut bytes = Vec::with_capacity(value.cbor_size_hint());
            ciborium::into_writer(&normal_value_to_cbor(value)?, &mut bytes)
                .map_err(|error| document::Error::CborEncode(error.to_string()))?;
            bytes
        };
        Ok(StoredField {
            name,
            kind: ValueKind::Lww,
            priority,
            value,
        })
    }
}

impl FieldCrdt for LwwField {
    fn ctype(&self) -> CType {
        CType::LwwRegister
    }

    fn replicates_ops(&self) -> bool {
        false
    }

    fn op(&self, input: NormalValue, _creating: bool) -> Result<FieldOp, FieldCrdtError> {
        Ok(FieldOp::Set(input))
    }

    fn apply(
        &self,
        _field: &FieldDescription,
        _current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError> {
        match op {
            FieldOp::Set(value) => Ok(value.clone()),
            FieldOp::Increment(_) => Err(FieldCrdtError::UnsupportedOp {
                crdt: CType::LwwRegister,
                op: "increment",
            }),
        }
    }

    fn write<'a>(&self, value: &'a NormalValue, _op: Option<&'a FieldOp>) -> FieldWrite<'a> {
        FieldWrite {
            data: value,
            merges: true,
        }
    }

    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError> {
        if data.is_empty() {
            return Ok(NormalValue::Null);
        }
        ciborium::from_reader(data).map_err(|error| FieldCrdtError::Decode {
            field: field.name.clone(),
            message: error.to_string(),
        })
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
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
        Self::stored(field.name.clone(), priority, value)
    }

    /// A block carries the register's state: its bytes are the state, and the
    /// tie-break orders them exactly as every replica received them.
    fn block_state(
        &self,
        field: &FieldDescription,
        priority: u64,
        data: &[u8],
    ) -> Result<StoredField, FieldCrdtError> {
        Ok(StoredField {
            name: field.name.clone(),
            kind: ValueKind::Lww,
            priority,
            value: data.to_vec(),
        })
    }

    fn materialize(&self, state: Part<'_>) -> Option<NormalValue> {
        if state.kind != ValueKind::Lww {
            return None;
        }
        if state.value.is_empty() {
            return Some(NormalValue::Null);
        }
        let mut reader = state.value;
        let value = ciborium::from_reader(&mut reader).ok()?;
        if !reader.is_empty() {
            return None;
        }
        cbor_to_normal_value(value).ok()
    }

    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult> {
        if incoming.priority < state.priority {
            return Some(MergeResult::RejectedLowerPriority {
                current: state.priority,
                incoming: incoming.priority,
            });
        }
        if incoming.priority == state.priority && incoming.value <= state.value {
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

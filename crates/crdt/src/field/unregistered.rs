//! The stand-in for a custom CRDT type this node does not have.
//!
//! A node keeps and replicates a definition whose field names a type it lacks,
//! but it cannot write, read or merge that field: it would have to guess the
//! type's merge, and a guess (LWW, say) diverges from every node that has it.
//! The stand-in answers every lookup for such a field and refuses everything,
//! so no caller can fall back to another type.

use defra_core::block::CrdtDelta;
use defra_core::block_delta::CustomDeltaPayload;
use document::{FieldOp, NormalValue};
use schema::{CType, CrdtName, FieldDescription};

use super::{DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite, MergeResult};
use crate::stored::{Part, StoredField};

#[derive(Clone, Copy)]
pub(super) struct UnregisteredField {
    name: CrdtName,
}

impl UnregisteredField {
    pub(super) fn new(name: CrdtName) -> Self {
        Self { name }
    }

    fn refused(&self) -> FieldCrdtError {
        FieldCrdtError::Unregistered { crdt: self.name }
    }
}

impl FieldCrdt for UnregisteredField {
    fn ctype(&self) -> CType {
        CType::Custom(self.name)
    }

    fn replicates_ops(&self) -> bool {
        false
    }

    fn op(&self, _input: NormalValue, _creating: bool) -> Result<FieldOp, FieldCrdtError> {
        Err(self.refused())
    }

    fn named_op(&self, _op: FieldOp) -> Result<FieldOp, FieldCrdtError> {
        Err(self.refused())
    }

    fn apply(
        &self,
        _field: &FieldDescription,
        _current: Option<&NormalValue>,
        _op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError> {
        Err(self.refused())
    }

    fn write<'value>(
        &self,
        value: &'value NormalValue,
        _op: Option<&'value FieldOp>,
    ) -> FieldWrite<'value> {
        FieldWrite {
            data: value,
            merges: false,
        }
    }

    fn block_data(
        &self,
        _value: &NormalValue,
        _op: Option<&FieldOp>,
    ) -> Result<Option<Vec<u8>>, FieldCrdtError> {
        Err(self.refused())
    }

    fn encode(&self, _data: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
        Err(self.refused())
    }

    fn decode(
        &self,
        _field: &FieldDescription,
        _data: &[u8],
    ) -> Result<NormalValue, FieldCrdtError> {
        Err(self.refused())
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Custom(CustomDeltaPayload {
            crdt: self.name.to_string(),
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
            nonce: None,
            collection_version_id: parts.collection_version_id.to_owned(),
            data: parts.data,
        })
    }

    fn state(
        &self,
        _field: &FieldDescription,
        _priority: u64,
        _value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        Err(self.refused())
    }

    fn block_state(
        &self,
        _field: &FieldDescription,
        _priority: u64,
        _data: &[u8],
    ) -> Result<StoredField, FieldCrdtError> {
        Err(self.refused())
    }

    fn materialize(&self, _state: Part<'_>) -> Option<NormalValue> {
        None
    }

    fn merge(&self, _state: &mut StoredField, _incoming: StoredField) -> Option<MergeResult> {
        None
    }

    fn folds_exactly(&self, _state: Part<'_>) -> bool {
        false
    }
}

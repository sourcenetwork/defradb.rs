//! A custom tagged CRDT type keeps the contract kit's laws: the native Json
//! type under a custom name, its parts of that custom kind.

use crdt::field::testing::check_field_crdt;
use crdt::field::{InPast, Tag, TaggedBlock, TaggedCrdt};
use crdt::stored::{Part, StoredField, ValueKind};
use crdt::{DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite, JsonField, MergeResult};
use defra_core::{CrdtDelta, CustomDeltaPayload};
use document::{FieldOp, NormalValue};
use schema::{CType, CrdtName, FieldDescription, FieldKind};
use serde_json::json;

const NAME: &str = "customjson";

fn name() -> CrdtName {
    CrdtName::new(NAME).unwrap()
}

/// [`JsonField`], named [`NAME`]: its states are of the custom kind, and
/// read and written as Json underneath.
struct CustomJson;

fn native(part: Part<'_>) -> Part<'_> {
    Part {
        kind: ValueKind::Json,
        ..part
    }
}

fn native_field(mut state: StoredField) -> StoredField {
    state.kind = ValueKind::Json;
    state
}

fn custom(mut state: StoredField) -> StoredField {
    state.kind = ValueKind::Custom(name());
    state
}

impl FieldCrdt for CustomJson {
    fn ctype(&self) -> CType {
        CType::Custom(name())
    }

    fn replicates_ops(&self) -> bool {
        JsonField.replicates_ops()
    }

    fn op(&self, input: NormalValue, creating: bool) -> Result<FieldOp, FieldCrdtError> {
        JsonField.op(input, creating)
    }

    fn named_op(&self, op: FieldOp) -> Result<FieldOp, FieldCrdtError> {
        JsonField.named_op(op)
    }

    fn apply(
        &self,
        field: &FieldDescription,
        current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError> {
        JsonField.apply(field, current, op)
    }

    fn write<'value>(
        &self,
        value: &'value NormalValue,
        op: Option<&'value FieldOp>,
    ) -> FieldWrite<'value> {
        JsonField.write(value, op)
    }

    fn block_data(
        &self,
        value: &NormalValue,
        op: Option<&FieldOp>,
    ) -> Result<Option<Vec<u8>>, FieldCrdtError> {
        JsonField.block_data(value, op)
    }

    fn encode(&self, data: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
        JsonField.encode(data)
    }

    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError> {
        JsonField.decode(field, data)
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Custom(CustomDeltaPayload {
            crdt: NAME.to_owned(),
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
            nonce: None,
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
        JsonField.state(field, priority, value).map(custom)
    }

    fn block_state(
        &self,
        field: &FieldDescription,
        priority: u64,
        data: &[u8],
    ) -> Result<StoredField, FieldCrdtError> {
        JsonField.block_state(field, priority, data).map(custom)
    }

    fn materialize(&self, state: Part<'_>) -> Option<NormalValue> {
        (state.kind == ValueKind::Custom(name()))
            .then(|| JsonField.materialize(native(state)))
            .flatten()
    }

    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult> {
        let mut native_state = native_field(state.clone());
        let result = JsonField.merge(&mut native_state, native_field(incoming))?;
        *state = custom(native_state);
        Some(result)
    }

    fn folds_exactly(&self, state: Part<'_>) -> bool {
        JsonField.folds_exactly(native(state))
    }

    fn tagged(&self) -> Option<&dyn TaggedCrdt> {
        Some(self)
    }

    fn blind_op(&self) -> bool {
        JsonField.blind_op()
    }
}

impl TaggedCrdt for CustomJson {
    fn op_id(&self, priority: u64, parents: &[Vec<u8>], data: &[u8]) -> Vec<u8> {
        JsonField.op_id(priority, parents, data)
    }

    fn tags(&self, field: &FieldDescription, state: Part<'_>) -> Result<Vec<Tag>, FieldCrdtError> {
        JsonField.tags(field, native(state))
    }

    fn write_data(
        &self,
        field: &FieldDescription,
        state: Option<Part<'_>>,
        op: &FieldOp,
        nonce: i64,
    ) -> Result<Vec<u8>, FieldCrdtError> {
        JsonField.write_data(field, state.map(native), op, nonce)
    }

    fn effect(
        &self,
        field: &FieldDescription,
        state: Option<Part<'_>>,
        data: &[u8],
        id: &[u8],
        priority: u64,
        in_past: &InPast<'_>,
    ) -> Result<StoredField, FieldCrdtError> {
        JsonField
            .effect(field, state.map(native), data, id, priority, in_past)
            .map(custom)
    }

    fn settle(&self, state: &mut StoredField) -> Option<()> {
        let mut native_state = native_field(state.clone());
        JsonField.settle(&mut native_state)?;
        *state = custom(native_state);
        Some(())
    }

    fn replay(
        &self,
        field: &FieldDescription,
        history: &[TaggedBlock],
    ) -> Result<Option<StoredField>, FieldCrdtError> {
        Ok(JsonField.replay(field, history)?.map(custom))
    }
}

#[test]
fn a_custom_tagged_type_keeps_the_contract() {
    let field = FieldDescription::new("1", "meta", FieldKind::json(), CType::Custom(name()));
    let values = |values: &[serde_json::Value]| -> Vec<NormalValue> {
        values.iter().cloned().map(NormalValue::Json).collect()
    };
    check_field_crdt(
        &CustomJson,
        &field,
        &values(&[json!({"a": 1, "b": {"c": 2}}), json!(5)]),
        &values(&[json!({"a": 2, "b": {"c": 2}}), json!({"b": {"d": [1, 2]}})]),
    );
}

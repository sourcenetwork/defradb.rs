use defra_core::block::{CrdtDelta, TextDeltaPayload};
use defra_core::TextOp;
use document::{FieldOp, NormalValue};
use rapidhash::RapidHashSet;
use schema::{CType, FieldDescription};

use super::{
    CausalBlock, CausalCrdt, DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite, LwwField,
    MergeResult,
};
use crate::fugue::{self, TextBlock};
use crate::stored::{Part, StoredField, ValueKind};

/// Collaborative text: blocks carry positional edits ([`TextOp`]), counted in
/// the block's parent version, and a field's value is the Fugue replay of its
/// history ([`fugue::replay`]). The document value stores the replayed text,
/// which each merge or write replaces whole. A write of null is a block whose
/// data is CBOR null ([`NIL`]): it clears its parent version, and the field
/// reads null while every head of its history is such a block.
pub struct TextField;

/// A Text block's data for a write of null: CBOR null.
const NIL: [u8; 1] = [0xf6];

impl TextField {
    /// The stored state of a text named `name` holding `value`: nothing for
    /// nil, its CBOR otherwise.
    pub fn stored(
        name: String,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        Ok(StoredField {
            kind: ValueKind::Text,
            ..LwwField::stored(name, priority, value)?
        })
    }
}

fn text_of(value: Option<&NormalValue>) -> &str {
    value.and_then(NormalValue::as_str).unwrap_or_default()
}

fn encode_ops(ops: &[TextOp]) -> Result<Vec<u8>, FieldCrdtError> {
    TextOp::encode_all(ops).map_err(|error| FieldCrdtError::Encode(error.to_string()))
}

fn decode_ops(field: &FieldDescription, data: &[u8]) -> Result<Vec<TextOp>, FieldCrdtError> {
    if data == NIL {
        return Ok(TextOp::replace_all(""));
    }
    TextOp::decode_all(data).map_err(|error| FieldCrdtError::Decode {
        field: field.name.clone(),
        message: error.to_string(),
    })
}

/// Whether a history's value is null: every head (a block no other block
/// names as a parent) writes null. Each such head cleared its parent version,
/// which holds every other block, so no character is left either way.
fn nil_heads(history: &[CausalBlock]) -> bool {
    let parents: RapidHashSet<&[u8]> = history
        .iter()
        .flat_map(|block| block.parents.iter().map(Vec::as_slice))
        .collect();
    history
        .iter()
        .filter(|block| !parents.contains(block.id.as_slice()))
        .all(|block| matches!(block.op, FieldOp::Set(NormalValue::Null)))
}

/// The value of a history whose replayed characters are `text`.
fn text_value(history: &[CausalBlock], text: String) -> NormalValue {
    if nil_heads(history) {
        NormalValue::Null
    } else {
        NormalValue::String(text)
    }
}

impl FieldCrdt for TextField {
    fn ctype(&self) -> CType {
        CType::Text
    }

    fn replicates_ops(&self) -> bool {
        true
    }

    /// A write of a whole text is a set, expressed as edits against the
    /// stored text once the write reads it ([`CausalCrdt::diff`]).
    fn op(&self, input: NormalValue, _creating: bool) -> Result<FieldOp, FieldCrdtError> {
        Ok(FieldOp::Set(input))
    }

    fn named_op(&self, op: FieldOp) -> Result<FieldOp, FieldCrdtError> {
        match op {
            FieldOp::Increment(_) => Err(FieldCrdtError::UnsupportedOp {
                crdt: CType::Text,
                op: "increment",
            }),
            op => Ok(op),
        }
    }

    fn apply(
        &self,
        _field: &FieldDescription,
        current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError> {
        match op {
            FieldOp::Set(value) => Ok(value.clone()),
            FieldOp::Edit(ops) => Ok(NormalValue::String(TextOp::apply_all(
                text_of(current),
                ops,
            ))),
            FieldOp::Increment(_) => Err(FieldCrdtError::UnsupportedOp {
                crdt: CType::Text,
                op: "increment",
            }),
        }
    }

    /// The document value takes the edited text whole.
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

    /// The edits a write makes; a write with none is the field's first, an
    /// insert of its whole text, and a nil first write makes no block.
    fn block_data(
        &self,
        value: &NormalValue,
        op: Option<&FieldOp>,
    ) -> Result<Option<Vec<u8>>, FieldCrdtError> {
        match op {
            Some(FieldOp::Set(NormalValue::Null)) => Ok(Some(NIL.to_vec())),
            Some(FieldOp::Edit(_)) if value.is_nil() => Ok(Some(NIL.to_vec())),
            Some(FieldOp::Edit(ops)) => encode_ops(ops).map(Some),
            Some(FieldOp::Set(value)) => {
                encode_ops(&TextOp::replace_all(text_of(Some(value)))).map(Some)
            }
            Some(FieldOp::Increment(_)) => Err(FieldCrdtError::UnsupportedOp {
                crdt: CType::Text,
                op: "increment",
            }),
            None if value.is_nil() => Ok(None),
            None => self.encode(value).map(Some),
        }
    }

    /// The block data of a first write of `data`: an insert of the whole text.
    fn encode(&self, data: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
        encode_ops(&TextOp::diff("", text_of(Some(data))))
    }

    /// The text a block's edits make of the empty text: a first block's value.
    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError> {
        if data == NIL {
            return Ok(NormalValue::Null);
        }
        Ok(NormalValue::String(TextOp::apply_all(
            "",
            &decode_ops(field, data)?,
        )))
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Text(TextDeltaPayload {
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
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
        Self::stored(field.name.clone(), priority, value)
    }

    fn materialize(&self, state: Part<'_>) -> Option<NormalValue> {
        if state.kind != ValueKind::Text {
            return None;
        }
        LwwField.materialize(Part {
            kind: ValueKind::Lww,
            ..state
        })
    }

    /// Every writer of the text read it validated first, so the later state
    /// replaces the earlier whatever its priority.
    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult> {
        state.value = incoming.value;
        state.priority = state.priority.max(incoming.priority);
        Some(MergeResult::Applied)
    }

    fn folds_exactly(&self, _state: Part<'_>) -> bool {
        true
    }

    fn causal(&self) -> Option<&dyn CausalCrdt> {
        Some(self)
    }
}

impl CausalCrdt for TextField {
    fn diff(
        &self,
        current: Option<&NormalValue>,
        value: &NormalValue,
    ) -> Result<FieldOp, FieldCrdtError> {
        Ok(FieldOp::Edit(TextOp::diff(
            text_of(current),
            text_of(Some(value)),
        )))
    }

    fn block_op(&self, field: &FieldDescription, data: &[u8]) -> Result<FieldOp, FieldCrdtError> {
        if data == NIL {
            return Ok(FieldOp::Set(NormalValue::Null));
        }
        decode_ops(field, data).map(FieldOp::Edit)
    }

    fn replay(
        &self,
        field: &FieldDescription,
        history: &[CausalBlock],
    ) -> Result<NormalValue, FieldCrdtError> {
        let blocks = history
            .iter()
            .map(|block| {
                let ops = match &block.op {
                    FieldOp::Edit(ops) => ops.clone(),
                    FieldOp::Set(value) => TextOp::replace_all(text_of(Some(value))),
                    FieldOp::Increment(_) => {
                        return Err(FieldCrdtError::UnsupportedOp {
                            crdt: CType::Text,
                            op: "increment",
                        })
                    }
                };
                Ok(TextBlock {
                    id: block.id.clone(),
                    parents: block.parents.clone(),
                    ops,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        fugue::replay(&blocks)
            .map(|text| text_value(history, text))
            .map_err(|error| FieldCrdtError::History {
                field: field.name.clone(),
                message: error.to_string(),
            })
    }
}

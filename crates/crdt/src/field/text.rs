use defra_core::block::{CrdtDelta, TextDeltaPayload};
use defra_core::{TextEdits, TextOp};
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

/// A block's data: `ops`, and the length of `text`, the text they leave.
fn encode_edits(ops: Vec<TextOp>, text: &str) -> Result<Vec<u8>, FieldCrdtError> {
    TextEdits {
        ops,
        len: text.chars().count() as u64,
    }
    .encode()
    .map_err(|error| FieldCrdtError::Encode(error.to_string()))
}

fn decode_edits(field: &FieldDescription, data: &[u8]) -> Result<TextEdits, FieldCrdtError> {
    if data == NIL {
        return Ok(TextEdits {
            ops: TextOp::replace_all(""),
            len: 0,
        });
    }
    TextEdits::decode(data).map_err(|error| FieldCrdtError::Decode {
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

/// The Fugue blocks of a field's causal blocks: a set restates the text.
fn text_blocks(history: &[CausalBlock]) -> Result<Vec<TextBlock>, FieldCrdtError> {
    history
        .iter()
        .map(|block| {
            let ops = match &block.op {
                FieldOp::Edit(ops) => ops.clone(),
                FieldOp::Set(value) => TextOp::replace_all(text_of(Some(value))),
                op => {
                    return Err(FieldCrdtError::UnsupportedOp {
                        crdt: CType::Text,
                        op: op.name(),
                    })
                }
            };
            Ok(TextBlock {
                id: block.id.clone(),
                parents: block.parents.clone(),
                ops,
            })
        })
        .collect()
}

fn history_error(field: &FieldDescription, error: impl std::fmt::Display) -> FieldCrdtError {
    FieldCrdtError::History {
        field: field.name.clone(),
        message: error.to_string(),
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
            op @ (FieldOp::Set(_) | FieldOp::Edit(_)) => Ok(op),
            op => Err(FieldCrdtError::UnsupportedOp {
                crdt: CType::Text,
                op: op.name(),
            }),
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
            op => Err(FieldCrdtError::UnsupportedOp {
                crdt: CType::Text,
                op: op.name(),
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
            Some(FieldOp::Edit(ops)) => encode_edits(ops.clone(), text_of(Some(value))).map(Some),
            Some(FieldOp::Set(value)) => {
                let text = text_of(Some(value));
                encode_edits(TextOp::replace_all(text), text).map(Some)
            }
            Some(op) => Err(FieldCrdtError::UnsupportedOp {
                crdt: CType::Text,
                op: op.name(),
            }),
            None if value.is_nil() => Ok(None),
            None => self.encode(value).map(Some),
        }
    }

    /// The block data of a first write of `data`: an insert of the whole text.
    fn encode(&self, data: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
        let text = text_of(Some(data));
        encode_edits(TextOp::diff("", text), text)
    }

    /// The text a block's edits make of the empty text: a first block's value.
    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError> {
        if data == NIL {
            return Ok(NormalValue::Null);
        }
        Ok(NormalValue::String(TextOp::apply_all(
            "",
            &decode_edits(field, data)?.ops,
        )))
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Text(TextDeltaPayload {
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
        decode_edits(field, data).map(|edits| FieldOp::Edit(edits.ops))
    }

    fn replay(
        &self,
        field: &FieldDescription,
        history: &[CausalBlock],
    ) -> Result<NormalValue, FieldCrdtError> {
        fugue::replay(&text_blocks(history)?)
            .map(|text| text_value(history, text))
            .map_err(|error| history_error(field, error))
    }

    /// Fugue over placeholders for the critical block's text, as many as the
    /// length its block records ([`fugue::merge_text`]).
    fn merge_from(
        &self,
        field: &FieldDescription,
        current: Option<&NormalValue>,
        critical: &[u8],
        after: &[CausalBlock],
        new: &[u8],
    ) -> Result<Option<NormalValue>, FieldCrdtError> {
        let Ok(base) = usize::try_from(decode_edits(field, critical)?.len) else {
            return Ok(None);
        };
        fugue::merge_text(text_of(current), base, &text_blocks(after)?, new)
            .map(|text| text.map(|text| text_value(after, text)))
            .map_err(|error| history_error(field, error))
    }

    /// A Text block records the length of the text it leaves.
    fn check_version(
        &self,
        field: &FieldDescription,
        data: &[u8],
        value: &NormalValue,
    ) -> Result<(), FieldCrdtError> {
        let recorded = decode_edits(field, data)?.len;
        let len = text_of(Some(value)).chars().count() as u64;
        if recorded == len {
            return Ok(());
        }
        Err(FieldCrdtError::Version {
            field: field.name.clone(),
            message: format!("it records {recorded} characters for its version, which has {len}"),
        })
    }
}

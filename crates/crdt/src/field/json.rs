use defra_core::block::{CrdtDelta, JsonDeltaPayload};
use defra_core::json_crdt::{self, Conflict, Frontier, JsonEdit, JsonOp, ReplayBlock};
use document::{FieldOp, NormalValue};
use schema::{CType, FieldDescription};
use serde_json::{Map, Value};

use super::{
    DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite, InPast, MergeResult, Tag, TaggedBlock,
    TaggedCrdt,
};
use crate::stored::{Part, StoredField, ValueKind};

/// A JSON value where every path is its own register
/// ([`json_crdt`](defra_core::json_crdt)). A block carries edits at paths,
/// and the field stores a [`Frontier`]: per path, the edits no later edit of
/// it or of a path above has seen. An edit removes only the edits in its
/// block's past, so the type is tagged ([`TaggedCrdt`]): a block's ID is a
/// digest of its priority, parents and edits ([`json_crdt::block_id`]).
pub struct JsonField;

/// A value as JSON, `None` when nil.
fn json_of(value: Option<&NormalValue>) -> Option<Value> {
    let value = value.filter(|value| !value.is_nil())?;
    match value.as_json() {
        Some(json) => Some(json.clone()),
        None => document::encoding::normal_value_to_json(value).ok(),
    }
}

fn normal(value: Option<Value>) -> NormalValue {
    value.map_or(NormalValue::Null, NormalValue::Json)
}

fn decode_error(field: &FieldDescription, error: impl std::fmt::Display) -> FieldCrdtError {
    FieldCrdtError::Decode {
        field: field.name.clone(),
        message: error.to_string(),
    }
}

fn frontier(field: &FieldDescription, state: Option<Part<'_>>) -> Result<Frontier, FieldCrdtError> {
    match state.filter(|state| !state.value.is_empty()) {
        Some(state) => Frontier::decode(state.value).map_err(|error| decode_error(field, error)),
        None => Ok(Frontier::default()),
    }
}

fn stored(name: &str, priority: u64, frontier: &Frontier) -> Result<StoredField, FieldCrdtError> {
    Ok(StoredField {
        name: name.to_owned(),
        kind: ValueKind::Json,
        priority,
        value: frontier
            .encode()
            .map_err(|error| FieldCrdtError::Encode(error.to_string()))?,
    })
}

fn encode_ops(ops: &[JsonOp]) -> Result<Vec<u8>, FieldCrdtError> {
    json_crdt::encode_ops(ops).map_err(|error| FieldCrdtError::Encode(error.to_string()))
}

fn encode_write(nonce: i64, ops: &[JsonOp]) -> Result<Vec<u8>, FieldCrdtError> {
    json_crdt::encode_write(nonce, ops).map_err(|error| FieldCrdtError::Encode(error.to_string()))
}

/// The map `node` holds, after making it an empty one if it held anything else.
fn map_of(node: &mut Value) -> &mut Map<String, Value> {
    if !node.is_object() {
        *node = Value::Object(Map::new());
    }
    match node {
        Value::Object(map) => map,
        _ => unreachable!("the node was just made a map"),
    }
}

/// `value` after `ops`, in order, each at its path. A map edit sets an empty
/// map, dropping what was below. A set through a value that is not a map
/// makes it one, as the merge does. A delete never creates
/// anything: one through a missing path or a value that is not a map changes
/// nothing.
fn apply_ops(value: &mut Option<Value>, ops: &[JsonOp]) {
    for op in ops {
        let Some((last, parents)) = op.path.split_last() else {
            *value = match &op.edit {
                JsonEdit::Set(new) => Some(new.clone()),
                JsonEdit::Map => Some(Value::Object(Map::new())),
                JsonEdit::Delete => None,
            };
            continue;
        };
        if let JsonEdit::Delete = op.edit {
            let mut node = value.as_mut();
            for key in parents {
                node = node
                    .and_then(Value::as_object_mut)
                    .and_then(|map| map.get_mut(key));
            }
            if let Some(map) = node.and_then(Value::as_object_mut) {
                map.remove(last);
            }
            continue;
        }
        let mut node = value.get_or_insert(Value::Null);
        for key in parents {
            node = map_of(node).entry(key.clone()).or_insert(Value::Null);
        }
        let map = map_of(node);
        match &op.edit {
            JsonEdit::Set(new) => {
                map.insert(last.clone(), new.clone());
            }
            JsonEdit::Map => {
                map.insert(last.clone(), Value::Object(Map::new()));
            }
            JsonEdit::Delete => {}
        }
    }
}

/// The value at `path` of `value`, if every key on the way is in a map.
fn value_at<'value>(value: Option<&'value Value>, path: &[String]) -> Option<&'value Value> {
    path.iter()
        .try_fold(value?, |node, key| node.as_object()?.get(key))
}

/// The ops that set `value` at `path` over `current`: an object sets each of
/// its leaves at its own path, making each path above a map where it is not
/// one already, and leaves every other key alone.
fn deep_set(path: &mut Vec<String>, current: Option<&Value>, value: &Value, ops: &mut Vec<JsonOp>) {
    let Value::Object(map) = value else {
        ops.push(JsonOp {
            path: path.clone(),
            edit: JsonEdit::Set(value.clone()),
        });
        return;
    };
    let current = current.and_then(Value::as_object);
    if current.is_none() {
        ops.push(JsonOp {
            path: path.clone(),
            edit: JsonEdit::Map,
        });
    }
    for (key, child) in map {
        path.push(key.clone());
        deep_set(path, current.and_then(|map| map.get(key)), child, ops);
        path.pop();
    }
}

/// Make the ops written so far in a block agree with an edit at `path` that
/// follows them. A block's ops are one edit to the frontier, none in the
/// past of another, so an edit does not remove what the same block wrote
/// below it, and a value the block wrote above it races the map the edit
/// implies. Read in order, the edit replaces what is at and below `path`
/// and makes each path above it a map: the earlier ops at or below `path`
/// are dropped, and a value above it becomes a map.
fn settle_earlier(ops: &mut Vec<JsonOp>, path: &[String]) {
    ops.retain(|op| !op.path.starts_with(path));
    for op in ops.iter_mut() {
        if path.starts_with(&op.path) && matches!(op.edit, JsonEdit::Set(_)) {
            op.edit = JsonEdit::Map;
        }
    }
}

/// The ops a block carries for `op` written over `current`: a set is the
/// diff from `current`, and a set of an object at a path is a set of each of
/// its leaves. Each op is read against the value the ones before it left.
fn block_ops(current: Option<&Value>, op: &FieldOp) -> Result<Vec<JsonOp>, FieldCrdtError> {
    let edits = match op {
        FieldOp::Set(value) => return Ok(json_crdt::diff(current, json_of(Some(value)).as_ref())),
        FieldOp::Paths(edits) => edits,
        op => {
            return Err(FieldCrdtError::UnsupportedOp {
                crdt: CType::Json,
                op: op.name(),
            })
        }
    };
    let mut working = current.cloned();
    let mut ops: Vec<JsonOp> = Vec::with_capacity(edits.len());
    for edit in edits {
        settle_earlier(&mut ops, &edit.path);
        let start = ops.len();
        match &edit.edit {
            JsonEdit::Set(value) => {
                let at = value_at(working.as_ref(), &edit.path);
                deep_set(&mut edit.path.clone(), at, value, &mut ops);
            }
            _ => ops.push(edit.clone()),
        }
        apply_ops(&mut working, &ops[start..]);
    }
    Ok(ops)
}

impl JsonField {
    /// The edits of a Json field's `state` that lost a race at a path to an
    /// edit that had not seen them. Refused for a field of any other type.
    pub fn conflicts(
        field: &FieldDescription,
        state: Option<Part<'_>>,
    ) -> Result<Vec<Conflict>, FieldCrdtError> {
        if field.crdt_type != CType::Json {
            return Err(FieldCrdtError::UnsupportedOp {
                crdt: field.crdt_type,
                op: "conflict queries",
            });
        }
        Ok(frontier(field, state)?.conflicts())
    }
}

impl FieldCrdt for JsonField {
    fn ctype(&self) -> CType {
        CType::Json
    }

    fn replicates_ops(&self) -> bool {
        true
    }

    /// A written value replaces the field's, as the diff from the stored one
    /// once the write reads it ([`TaggedCrdt::write_data`]).
    fn op(&self, input: NormalValue, _creating: bool) -> Result<FieldOp, FieldCrdtError> {
        Ok(FieldOp::Set(input))
    }

    /// A named set sets each leaf of the value at its path and leaves every
    /// other key alone; path edits are taken as they are.
    fn named_op(&self, op: FieldOp) -> Result<FieldOp, FieldCrdtError> {
        match op {
            FieldOp::Set(value) => Ok(FieldOp::Paths(vec![JsonOp {
                path: Vec::new(),
                edit: JsonEdit::Set(json_of(Some(&value)).unwrap_or(Value::Null)),
            }])),
            op @ FieldOp::Paths(_) => Ok(op),
            op => Err(FieldCrdtError::UnsupportedOp {
                crdt: CType::Json,
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
        let mut value = json_of(current);
        let ops = block_ops(value.as_ref(), op)?;
        apply_ops(&mut value, &ops);
        Ok(normal(value))
    }

    /// The document value takes the write as its prepared operand
    /// ([`FieldOp::Operand`]), never as a value.
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

    /// The edits a write makes: those it prepared, or, for a first write,
    /// its value written over an absent field. A nil first write makes no
    /// block.
    fn block_data(
        &self,
        value: &NormalValue,
        op: Option<&FieldOp>,
    ) -> Result<Option<Vec<u8>>, FieldCrdtError> {
        match op {
            Some(FieldOp::Operand { data, .. }) => Ok(Some(data.clone())),
            Some(op) => encode_ops(&block_ops(None, op)?).map(Some),
            None if value.is_nil() => Ok(None),
            None => self.encode(value).map(Some),
        }
    }

    /// The edits that write `data` over an absent field.
    fn encode(&self, data: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
        encode_ops(&json_crdt::diff(None, json_of(Some(data)).as_ref()))
    }

    /// The value a block's edits make of an absent field.
    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError> {
        let ops = json_crdt::decode_ops(data).map_err(|error| decode_error(field, error))?;
        let mut value = None;
        apply_ops(&mut value, &ops);
        Ok(normal(value))
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Json(JsonDeltaPayload {
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
            collection_version_id: parts.collection_version_id.to_owned(),
            data: parts.data,
        })
    }

    /// The frontier of `value` written by one root block at `priority`, the
    /// block a create writes.
    fn state(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        let ops = json_crdt::diff(None, json_of(Some(value)).as_ref());
        let data = encode_ops(&ops)?;
        let frontier = Frontier::default().local_delta(
            &ops,
            &json_crdt::block_id(priority, &[], &data),
            priority,
        );
        stored(&field.name, priority, &frontier)
    }

    fn materialize(&self, state: Part<'_>) -> Option<NormalValue> {
        if state.kind != ValueKind::Json {
            return None;
        }
        if state.value.is_empty() {
            return Some(NormalValue::Null);
        }
        Some(normal(Frontier::decode(state.value).ok()?.materialize()))
    }

    /// Operands compose: the later drops what it removes and adds its
    /// entries, keeping what both remove for a base not merged yet.
    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult> {
        let decode = |bytes: &[u8]| {
            if bytes.is_empty() {
                Some(Frontier::default())
            } else {
                Frontier::decode(bytes).ok()
            }
        };
        let mut frontier = decode(&state.value)?;
        frontier.compose(decode(&incoming.value)?);
        state.value = frontier.encode().ok()?;
        state.priority = state.priority.max(incoming.priority);
        Some(MergeResult::Applied)
    }

    fn folds_exactly(&self, _state: Part<'_>) -> bool {
        true
    }

    fn tagged(&self) -> Option<&dyn TaggedCrdt> {
        Some(self)
    }
}

impl TaggedCrdt for JsonField {
    fn op_id(&self, priority: u64, parents: &[Vec<u8>], data: &[u8]) -> Vec<u8> {
        json_crdt::block_id(priority, parents, data)
    }

    fn tags(&self, field: &FieldDescription, state: Part<'_>) -> Result<Vec<Tag>, FieldCrdtError> {
        Ok(frontier(field, Some(state))?
            .entries
            .into_iter()
            .map(|entry| Tag {
                priority: entry.priority,
                id: entry.block,
            })
            .collect())
    }

    fn write_data(
        &self,
        field: &FieldDescription,
        state: Option<Part<'_>>,
        op: &FieldOp,
        nonce: i64,
    ) -> Result<Vec<u8>, FieldCrdtError> {
        let frontier = frontier(field, state)?;
        let current = frontier.materialize();
        let ops = block_ops(current.as_ref(), op)?;
        // A map the stored value reads as may only be implied by an edit
        // below it, which a diff of values cannot see: when the diff's edits
        // do not leave the written value, the write rewrites it whole.
        if let FieldOp::Set(value) = op {
            let target = json_of(Some(value));
            let mut written = frontier.clone();
            written.compose(frontier.local_delta(&ops, &[], 0));
            if written.materialize() != target {
                return encode_write(nonce, &json_crdt::diff(None, target.as_ref()));
            }
        }
        encode_write(nonce, &ops)
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
        let ops = json_crdt::decode_ops(data).map_err(|error| decode_error(field, error))?;
        let delta = frontier(field, state)?.delta(&ops, id, priority, in_past);
        stored(&field.name, priority, &delta)
    }

    /// The frontier of the whole DAG, each block reading only the entries
    /// its edits reach ([`json_crdt::replay`]).
    fn replay(
        &self,
        field: &FieldDescription,
        history: &[TaggedBlock],
    ) -> Result<Option<StoredField>, FieldCrdtError> {
        let Some(priority) = history.iter().map(|block| block.priority).max() else {
            return Ok(None);
        };
        let blocks = history
            .iter()
            .map(|block| ReplayBlock {
                cid: block.cid.clone(),
                parents: block.parents.clone(),
                priority: block.priority,
                data: block.data.clone(),
            })
            .collect();
        let frontier = json_crdt::replay(blocks).map_err(|error| FieldCrdtError::History {
            field: field.name.clone(),
            message: error.to_string(),
        })?;
        stored(&field.name, priority, &frontier).map(Some)
    }

    fn settle(&self, state: &mut StoredField) -> Option<()> {
        if state.value.is_empty() {
            return Some(());
        }
        let mut frontier = Frontier::decode(&state.value).ok()?;
        frontier.settle();
        state.value = frontier.encode().ok()?;
        Some(())
    }
}

//! Errors in a patch's operations themselves, as Go reports them, before the
//! patch is applied. What the patched collections may look like is checked
//! afterwards by `schema::definition_validation`.

use crate::error::{Error, Result};
use crate::txn::DbTxn;
use rapidhash::RapidHashMap;
use schema::{CollectionVersion, FieldKind, ScalarKind};
use serde_json::Value;
use storage::corekv::Store;

const VERSION_ID: &str = "VersionID";
const FIELDS: &str = "Fields";
const KIND: &str = "Kind";
const APPEND: &str = "-";

/// The operations of a patch: a non-empty JSON array.
pub(crate) fn operations(patch: &str) -> Result<Vec<Value>> {
    let patch: Value = serde_json::from_str(patch).map_err(|e| invalid(e.to_string()))?;
    let Value::Array(ops) = patch else {
        return Err(invalid("patch must be a JSON array"));
    };
    if ops.is_empty() {
        return Err(invalid("patch array is empty"));
    }
    Ok(ops)
}

/// Check an operation, as given (`raw`) and after substitution (`op`),
/// against the versions the operations before it left.
pub(super) fn check_operation(
    raw: &Value,
    op: &Value,
    versions: &RapidHashMap<String, CollectionVersion>,
) -> Result<()> {
    let kind = op.get("op").and_then(Value::as_str).unwrap_or_default();
    let path = op
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("patch op missing 'path' field"))?;
    let tokens = tokens(path);
    check_known_collection(kind, &tokens, versions)?;
    check_version_id(kind, &tokens, op.get("value"), versions)?;
    check_added_field_name(kind, raw, &tokens, op.get("value"))?;
    check_append_target(kind, &tokens, versions)?;
    check_field_kinds(kind, &tokens, op.get("value"))
}

/// Go's `validateVersionID` and `validateCollectionID`: a version's IDs
/// are the CIDs of blocks already in the blockstore, so a version the
/// patch adds under an ID it made up is refused. Stored versions always
/// pass, so only added ones are checked; placeholders have no block.
pub(super) async fn check_added_ids<S>(txn: &DbTxn<S>, version: &CollectionVersion) -> Result<()>
where
    S: Store,
{
    if version.is_placeholder {
        return Ok(());
    }
    let blockstore = txn.blockstore()?;
    for (name, value) in [
        ("VersionID", &version.version_id),
        ("CollectionID", &version.collection_id),
    ] {
        let cid = cid::Cid::try_from(value.as_str())
            .map_err(|e| invalid(format!("invalid cid: {e}. {name}: {value}")))?;
        if !blockstore
            .has(&cid.to_bytes())
            .await
            .map_err(Error::Storage)?
        {
            return Err(invalid(format!(
                "unknown CID, collection ids cannot be manually defined. {name}: {value}"
            )));
        }
    }
    Ok(())
}

fn invalid<M>(message: M) -> Error
where
    M: Into<String>,
{
    Error::InvalidPatch(message.into())
}

/// A pointer's unescaped tokens; `""` is the root.
fn tokens(pointer: &str) -> Vec<String> {
    pointer
        .split('/')
        .skip(1)
        .map(|t| t.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// Go's patch library refuses to add inside a collection that does not
/// exist, in the versions as the operations before this one left them.
fn check_known_collection(
    kind: &str,
    tokens: &[String],
    versions: &RapidHashMap<String, CollectionVersion>,
) -> Result<()> {
    match tokens {
        [collection, _, ..] if kind == "add" && !versions.contains_key(collection) => {
            Err(invalid("add operation does not apply: doc is missing path"))
        }
        _ => Ok(()),
    }
}

/// A version's ID is its content's CID, so a patch never sets or removes it.
fn check_version_id(
    kind: &str,
    tokens: &[String],
    value: Option<&Value>,
    versions: &RapidHashMap<String, CollectionVersion>,
) -> Result<()> {
    let [collection, member] = tokens else {
        return Ok(());
    };
    // A key that is no version's ID is a collection the patch is adding.
    let Some(host) = versions
        .get(collection)
        .filter(|v| v.version_id == *collection)
    else {
        return Ok(());
    };
    if member != VERSION_ID {
        return Ok(());
    }
    match kind {
        "remove" => Err(invalid("invalid cid: cid too short. VersionID: ")),
        "add" | "replace" => {
            let version_id = value.and_then(Value::as_str).unwrap_or_default();
            if version_id.is_empty() {
                return Err(invalid("collection ID cannot be empty"));
            }
            if cid::Cid::try_from(version_id).is_err() {
                return Err(invalid(format!(
                    "invalid cid: selected encoding not supported. VersionID: {version_id}"
                )));
            }
            Err(invalid(match versions.get(version_id) {
                None => "unknown CID, collection ids cannot be manually defined",
                Some(other) if other.collection_id == host.collection_id => {
                    "collection sources cannot be added or removed."
                }
                Some(_) => "collection source must belong to host collection.",
            }))
        }
        _ => Ok(()),
    }
}

/// `add /<Collection>/Fields/<name>` appends a field of that name, so the
/// field it adds must not name itself otherwise.
fn check_added_field_name(
    kind: &str,
    raw: &Value,
    tokens: &[String],
    value: Option<&Value>,
) -> Result<()> {
    let appends = matches!(tokens, [_, fields, end] if fields == FIELDS && end == APPEND);
    if kind != "add" || !appends {
        return Ok(());
    }
    let raw_path = raw.get("path").and_then(Value::as_str).unwrap_or_default();
    let Some(index) = raw_path.rsplit('/').next() else {
        return Ok(());
    };
    if index == APPEND {
        return Ok(());
    }
    match value.and_then(|v| v.get("Name")).and_then(Value::as_str) {
        Some(name) if !name.is_empty() && name != index => Err(invalid(format!(
            "the index used does not match the given name. index: {index}, name: {name}"
        ))),
        _ => Ok(()),
    }
}

/// `-` appends only to an array. Inside a version, on an object, it names a
/// key `-`, which Go refuses as it decodes the patched version.
fn check_append_target(
    kind: &str,
    tokens: &[String],
    versions: &RapidHashMap<String, CollectionVersion>,
) -> Result<()> {
    let Some((last, parent)) = tokens.split_last() else {
        return Ok(());
    };
    if kind != "add" || last != APPEND || parent.is_empty() {
        return Ok(());
    }
    let Some(version) = versions.get(&parent[0]) else {
        return Ok(());
    };
    let json = serde_json::to_value(version)
        .map_err(|e| Error::collection_schema_json("failed to serialize schema to JSON", e))?;
    let target = parent[1..]
        .iter()
        .try_fold(&json, |node, token| match node {
            Value::Object(map) => map.get(token),
            Value::Array(items) => token.parse::<usize>().ok().and_then(|i| items.get(i)),
            _ => None,
        });
    if matches!(target, Some(Value::Object(_))) {
        return Err(invalid("json: unknown field \"-\""));
    }
    Ok(())
}

/// A numeric `Kind` the patch writes must name a kind. Go refuses an unknown
/// number as it decodes the patched version; Rust's decoding reads any
/// unknown number as kind 0, so the number is checked here, where the patch
/// still has it.
fn check_field_kinds(kind: &str, tokens: &[String], value: Option<&Value>) -> Result<()> {
    if !matches!(kind, "add" | "replace") {
        return Ok(());
    }
    let Some(value) = value else {
        return Ok(());
    };
    let kinds: Vec<&Value> = match tokens {
        [_, fields, _, leaf] if fields == FIELDS && leaf == KIND => vec![value],
        [_, fields, _] if fields == FIELDS => value.get(KIND).into_iter().collect(),
        [_, fields] if fields == FIELDS => field_kinds(value),
        [_] => value.get(FIELDS).map(field_kinds).unwrap_or_default(),
        _ => Vec::new(),
    };
    match kinds
        .into_iter()
        .filter_map(Value::as_u64)
        .find(|number| !is_known_kind(*number))
    {
        Some(number) => Err(invalid(format!(
            "no type found for given name. Type: {number}"
        ))),
        None => Ok(()),
    }
}

/// The `Kind` of each field in a JSON array of fields.
fn field_kinds(fields: &Value) -> Vec<&Value> {
    fields
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| field.get(KIND))
        .collect()
}

/// Whether `number` is a kind: 0, the unset kind, or a number
/// `FieldKind::from_numeric_kind` maps to something other than unset.
fn is_known_kind(number: u64) -> bool {
    number == 0
        || u8::try_from(number).is_ok_and(|number| {
            FieldKind::from_numeric_kind(number) != FieldKind::Scalar(ScalarKind::None)
        })
}

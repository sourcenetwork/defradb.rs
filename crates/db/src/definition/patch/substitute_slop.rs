//! Rewrites a JSON Patch operation's human-friendly references into the
//! values the collection version map stores.
//!
//! The patch works on a map of collection versions keyed by version ID. Three
//! rewrites apply, in `path` and `from`:
//! - a collection named by its name becomes the version ID of its active
//!   version, found first among the stored versions (an earlier operation may
//!   have moved the name away) and then among the current ones;
//! - a field addressed by name in `/<Collection>/Fields/<name>/...` becomes its
//!   index in the current version, as the operations before it left it;
//!   adding `/<Collection>/Fields/<new name>` appends a field of that name;
//! - a `value` written to `/<Collection>/Fields/<index>/Kind` or `.../Typ`
//!   given by its GQL name (`"Boolean"`, `"[Int!]"`, `"lww"`) becomes its
//!   numeric code.
//!
//! Anything already in disk form, or not recognised, passes through unchanged
//! so later validation reports it.

use crate::error::{Error, Result};
use rapidhash::RapidHashMap;
use schema::{CType, CollectionVersion, FieldKind};
use serde_json::Value;

const FIELDS: &str = "Fields";
const KIND: &str = "Kind";
const TYP: &str = "Typ";
const NAME: &str = "Name";
const APPEND: &str = "-";

/// Return `op` with collection names and field names in its pointers
/// replaced by version IDs and field indexes, and Kind and Typ names in its
/// value replaced by their numeric codes.
///
/// `stored` holds the versions before the patch and `current` the versions
/// as the patch's earlier operations left them. Collection names resolve
/// against `stored`, then `current`; field names against `current` only, so
/// an index reflects the fields an earlier operation removed or inserted.
pub(super) fn substitute(
    op: &Value,
    stored: &RapidHashMap<String, CollectionVersion>,
    current: &RapidHashMap<String, CollectionVersion>,
) -> Result<Value> {
    let mut op = op.clone();
    let members = op
        .as_object_mut()
        .ok_or_else(|| invalid("operation must be a JSON object"))?;
    let is_add = members.get("op").and_then(Value::as_str) == Some("add");

    let mut leaf = None;
    let mut added_name = None;
    for member in ["path", "from"] {
        let Some(pointer) = members.get(member) else {
            continue;
        };
        let pointer = pointer
            .as_str()
            .ok_or_else(|| invalid(format!("{member:?} member must be a string")))?;
        let mut tokens = parse_pointer(pointer)?;
        substitute_collection(&mut tokens, stored);
        substitute_collection(&mut tokens, current);
        let version = tokens.first().and_then(|id| current.get(id));
        if let Some(version) = version {
            let appends = is_add && member == "path";
            let appended = substitute_field_index(&mut tokens, version, appends);
            if member == "path" {
                added_name = appended;
            }
        }
        if member == "path" {
            leaf = field_leaf(&tokens);
        }
        members.insert(member.to_string(), Value::String(join_pointer(&tokens)));
    }

    if let (Some(name), Some(Value::Object(field))) = (added_name, members.get_mut("value")) {
        let unnamed = field
            .get(NAME)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty);
        if unnamed {
            field.insert(NAME.to_string(), Value::String(name));
        }
    }
    if let (Some(leaf), Some(value)) = (leaf, members.get_mut("value")) {
        substitute_value(leaf, value);
    }
    Ok(op)
}

fn invalid<M>(message: M) -> Error
where
    M: Into<String>,
{
    Error::InvalidPatch(message.into())
}

/// Replace a collection name in the first token with its active version's ID.
fn substitute_collection(
    tokens: &mut [String],
    versions: &RapidHashMap<String, CollectionVersion>,
) {
    let Some(collection) = tokens.first_mut() else {
        return;
    };
    if versions.contains_key(collection.as_str()) {
        return;
    }
    if let Some(active) = versions
        .values()
        .find(|v| v.is_active && v.name == *collection)
    {
        *collection = active.version_id.clone();
    }
}

/// Which code-valued field member a path addresses.
#[derive(Clone, Copy)]
enum Leaf {
    Kind,
    Typ,
}

fn field_leaf(tokens: &[String]) -> Option<Leaf> {
    match tokens {
        [_, fields, _, leaf] if fields == FIELDS => match leaf.as_str() {
            KIND => Some(Leaf::Kind),
            TYP => Some(Leaf::Typ),
            _ => None,
        },
        _ => None,
    }
}

/// Replace a field name in `/<Collection>/Fields/<name>/...` with its index.
/// When `appends` and the pointer is exactly `/<Collection>/Fields/<name>` of
/// a name no field has, the field is appended: the token becomes `-` and the
/// name is returned for the added field.
fn substitute_field_index(
    tokens: &mut [String],
    version: &CollectionVersion,
    appends: bool,
) -> Option<String> {
    let is_whole_field = tokens.len() == 3;
    let [_, fields, field, ..] = tokens else {
        return None;
    };
    if fields != FIELDS || field == APPEND || field.parse::<usize>().is_ok() {
        return None;
    }
    if let Some(index) = version.fields.iter().position(|f| f.name == *field) {
        *field = index.to_string();
        return None;
    }
    if appends && is_whole_field {
        return Some(std::mem::replace(field, APPEND.to_string()));
    }
    None
}

fn substitute_value(leaf: Leaf, value: &mut Value) {
    let Value::String(name) = value else {
        return;
    };
    let code = match leaf {
        Leaf::Kind => kind_code(name),
        Leaf::Typ => typ_code(name),
    };
    if let Some(code) = code {
        *value = Value::from(code);
    }
}

/// A GQL kind name (`"Boolean"`, `"[Int!]"`) as its scalar or scalar-array
/// code; relation and named kinds have no code and stay as given.
fn kind_code(name: &str) -> Option<u8> {
    match serde_json::from_value(Value::from(name)).ok()? {
        FieldKind::Scalar(kind) => Some(kind as u8),
        FieldKind::ScalarArray(kind) => Some(kind as u8),
        _ => None,
    }
}

/// A GQL CRDT name (`"lww"`, `"pncounter"`) as its code.
fn typ_code(name: &str) -> Option<u8> {
    match serde_json::from_value(Value::from(name)).ok()? {
        CType::Custom(_) | CType::Unknown(_) => None,
        ctype => Some(CType::to_u8(ctype)),
    }
}

/// Split a JSON Pointer (RFC 6901) into unescaped tokens; `""` is the root.
fn parse_pointer(pointer: &str) -> Result<Vec<String>> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    let rest = pointer
        .strip_prefix('/')
        .ok_or_else(|| invalid(format!("pointer {pointer:?} must start with '/'")))?;
    rest.split('/')
        .map(|token| unescape(token, pointer))
        .collect()
}

fn unescape(token: &str, pointer: &str) -> Result<String> {
    let mut out = String::with_capacity(token.len());
    let mut chars = token.chars();
    while let Some(c) = chars.next() {
        if c != '~' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('0') => out.push('~'),
            Some('1') => out.push('/'),
            _ => {
                return Err(invalid(format!(
                    "pointer {pointer:?} has an invalid '~' escape"
                )))
            }
        }
    }
    Ok(out)
}

fn join_pointer(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|t| format!("/{}", t.replace('~', "~0").replace('/', "~1")))
        .collect()
}

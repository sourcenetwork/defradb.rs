//! A JSON Patch (RFC 6902) engine over a map of collection versions.
//!
//! The patched document is a JSON object of collection versions keyed by
//! version ID, each in its Go JSON form, so `/<VersionID>/Name` addresses a
//! version's name and `/<VersionID>/Fields/-` appends a field. A key that is
//! no version ID names a new collection by name. The patch is applied to a
//! copy: either every operation succeeds and the patched map is returned, or
//! the input is untouched and an error names the failing operation.

use crate::error::{Error, Result};
use rapidhash::RapidHashMap;
use schema::CollectionVersion;
use serde_json::{Map, Value};

/// Apply a JSON Patch document to a map of collection versions keyed by
/// version ID.
///
/// `patch` must be an array of operation objects (`add`, `remove`, `replace`,
/// `move`, `copy`, `test`). Operations run in order; the first failure aborts
/// the whole patch with [`Error::InvalidPatch`], as does a result whose
/// entries no longer deserialize into [`CollectionVersion`]s.
pub(super) fn apply_patch(
    patch: &Value,
    versions: &RapidHashMap<String, CollectionVersion>,
) -> Result<RapidHashMap<String, CollectionVersion>> {
    let ops = patch
        .as_array()
        .ok_or_else(|| invalid("patch must be a JSON array of operations"))?;
    let mut doc = Value::Object(
        versions
            .iter()
            .map(|(id, version)| Ok((id.clone(), go_json(version)?)))
            .collect::<Result<Map<String, Value>>>()?,
    );
    for (index, op) in ops.iter().enumerate() {
        apply_operation(&mut doc, op).map_err(|e| invalid(format!("operation {index}: {e}")))?;
    }
    let Value::Object(entries) = doc else {
        return Err(invalid("patched value is not a map of collection versions"));
    };
    entries
        .into_iter()
        .map(|(key, value)| {
            let version = serde_json::from_value(value).map_err(|e| {
                invalid(format!(
                    "patched value {key:?} is not a collection version: {e}"
                ))
            })?;
            Ok((key, version))
        })
        .collect()
}

/// A version's JSON as Go writes it: Go always writes these keys, empty or
/// not, where Rust omits the empty ones.
fn go_json(version: &CollectionVersion) -> Result<Value> {
    let mut json = serde_json::to_value(version).map_err(|e| invalid(e.to_string()))?;
    if let Value::Object(members) = &mut json {
        for key in ["Indexes", "VectorEmbeddings"] {
            members
                .entry(key)
                .or_insert_with(|| Value::Array(Vec::new()));
        }
        for key in ["CollectionSet", "Query", "PreviousVersion", "Policy"] {
            members.entry(key).or_insert(Value::Null);
        }
    }
    Ok(json)
}

fn invalid<M>(message: M) -> Error
where
    M: Into<String>,
{
    Error::InvalidPatch(message.into())
}

type OpResult<T> = std::result::Result<T, String>;

fn apply_operation(doc: &mut Value, op: &Value) -> OpResult<()> {
    let op = op
        .as_object()
        .ok_or_else(|| "operation must be a JSON object".to_string())?;
    let kind = string_member(op, "op")?;
    let path = parse_pointer(string_member(op, "path")?)?;
    match kind {
        "add" => add(doc, &path, value_member(op)?.clone()),
        "remove" => remove(doc, &path).map(drop),
        "replace" => replace(doc, &path, value_member(op)?.clone()),
        "move" => {
            let from = parse_pointer(string_member(op, "from")?)?;
            move_value(doc, &from, &path)
        }
        "copy" => {
            let from = parse_pointer(string_member(op, "from")?)?;
            let value = get(doc, &from)?.clone();
            add(doc, &path, value)
        }
        "test" => {
            // Go's patch library reads a missing path as a failed test.
            let expected = value_member(op)?;
            if get(doc, &path).is_ok_and(|actual| json_equal(actual, expected)) {
                Ok(())
            } else {
                Err(format!(
                    "testing value {} failed: test failed",
                    display(&path)
                ))
            }
        }
        other => Err(format!("unknown operation {other:?}")),
    }
}

fn string_member<'op>(op: &'op Map<String, Value>, name: &str) -> OpResult<&'op str> {
    op.get(name)
        .ok_or_else(|| format!("missing {name:?} member"))?
        .as_str()
        .ok_or_else(|| format!("{name:?} member must be a string"))
}

fn value_member(op: &Map<String, Value>) -> OpResult<&Value> {
    op.get("value")
        .ok_or_else(|| "missing \"value\" member".to_string())
}

/// Split a JSON Pointer into its unescaped reference tokens; `""` is the root.
fn parse_pointer(pointer: &str) -> OpResult<Vec<String>> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    let rest = pointer
        .strip_prefix('/')
        .ok_or_else(|| format!("pointer {pointer:?} must start with '/'"))?;
    rest.split('/')
        .map(|token| unescape(token, pointer))
        .collect()
}

fn unescape(token: &str, pointer: &str) -> OpResult<String> {
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
            _ => return Err(format!("pointer {pointer:?} has an invalid '~' escape")),
        }
    }
    Ok(out)
}

fn display(path: &[String]) -> String {
    path.iter()
        .map(|t| format!("/{}", t.replace('~', "~0").replace('/', "~1")))
        .collect()
}

/// Parse an array index token: `0` or digits without a leading zero.
fn array_index(token: &str) -> OpResult<usize> {
    let well_formed = !token.is_empty()
        && token.bytes().all(|b| b.is_ascii_digit())
        && (token == "0" || !token.starts_with('0'));
    if !well_formed {
        return Err(format!("{token:?} is not an array index"));
    }
    token
        .parse()
        .map_err(|_| format!("array index {token:?} is out of range"))
}

fn get<'doc>(doc: &'doc Value, path: &[String]) -> OpResult<&'doc Value> {
    path.iter()
        .try_fold(doc, |current, token| match current {
            Value::Object(map) => map.get(token),
            Value::Array(items) => array_index(token).ok().and_then(|i| items.get(i)),
            _ => None,
        })
        .ok_or_else(|| format!("path {} does not exist", display(path)))
}

fn get_mut<'doc>(doc: &'doc mut Value, path: &[String]) -> OpResult<&'doc mut Value> {
    let mut current = doc;
    for token in path {
        current = match current {
            Value::Object(map) => map.get_mut(token),
            Value::Array(items) => match array_index(token) {
                Ok(i) => items.get_mut(i),
                Err(_) => None,
            },
            _ => None,
        }
        .ok_or_else(|| format!("path {} does not exist", display(path)))?;
    }
    Ok(current)
}

/// The container holding the target of `path`, and the target's token.
fn parent_mut<'doc, 'path>(
    doc: &'doc mut Value,
    path: &'path [String],
) -> OpResult<(&'doc mut Value, &'path str)> {
    let (last, parent) = path
        .split_last()
        .ok_or_else(|| "the root has no parent".to_string())?;
    Ok((get_mut(doc, parent)?, last))
}

fn add(doc: &mut Value, path: &[String], value: Value) -> OpResult<()> {
    if path.is_empty() {
        *doc = value;
        return Ok(());
    }
    let (parent, token) = parent_mut(doc, path)?;
    match parent {
        Value::Object(map) => {
            map.insert(token.to_string(), value);
            Ok(())
        }
        Value::Array(items) => {
            let index = if token == "-" {
                items.len()
            } else {
                array_index(token)?
            };
            if index > items.len() {
                return Err(format!(
                    "array index {index} is past the end at {}",
                    display(path)
                ));
            }
            items.insert(index, value);
            Ok(())
        }
        _ => Err(format!("cannot add into a scalar at {}", display(path))),
    }
}

fn remove(doc: &mut Value, path: &[String]) -> OpResult<Value> {
    if path.is_empty() {
        return Err("cannot remove the root".to_string());
    }
    let missing = || format!("path {} does not exist", display(path));
    let (parent, token) = parent_mut(doc, path)?;
    match parent {
        Value::Object(map) => map.remove(token).ok_or_else(missing),
        Value::Array(items) => {
            let index = array_index(token)?;
            if index >= items.len() {
                return Err(missing());
            }
            Ok(items.remove(index))
        }
        _ => Err(missing()),
    }
}

fn replace(doc: &mut Value, path: &[String], value: Value) -> OpResult<()> {
    *get_mut(doc, path)? = value;
    Ok(())
}

fn move_value(doc: &mut Value, from: &[String], path: &[String]) -> OpResult<()> {
    if from == path {
        return get(doc, from).map(drop);
    }
    if path.starts_with(from) {
        return Err(format!(
            "cannot move {} into its own child {}",
            display(from),
            display(path)
        ));
    }
    let value = remove(doc, from)?;
    add(doc, path, value)
}

/// RFC 6902 equality: numbers compare by value, objects ignore member order.
fn json_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_i64(), y.as_i64()) {
            (Some(x), Some(y)) => x == y,
            _ => match (x.as_u64(), y.as_u64()) {
                (Some(x), Some(y)) => x == y,
                _ => x.as_f64() == y.as_f64(),
            },
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| json_equal(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| json_equal(v, w)))
        }
        _ => a == b,
    }
}

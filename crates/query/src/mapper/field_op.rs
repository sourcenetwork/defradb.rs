//! Explicit CRDT ops in an update's input.
//!
//! `update_X(input: { insert: { body: { at: 4, value: "quick " } } })` names an
//! op, then the fields it applies to. Any other key is a field written with an
//! implicit `set`, so `input: { name: "John" }` keeps its meaning. A field named
//! like an op is written through an explicit `set: { set: ... }`.

use defra_core::TextOp;
use rapidhash::{HashMapExt, RapidHashMap};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::error::{QueryError, Result};

/// The op names an update input reads as ops rather than fields.
pub const FIELD_OPS: [&str; 5] = ["set", "increment", "insert", "delete", "replace"];

/// One explicit op on a field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldOp {
    /// Write this value. On a counter, the counter moves to it.
    Set(JsonValue),
    /// Add this amount to a counter.
    Increment(JsonValue),
    /// Edit a Text field, in character positions of its current text.
    Text(Vec<TextOp>),
}

/// Split an update input into its implicitly set fields and its explicit ops.
pub fn split_update_input(
    input: RapidHashMap<String, JsonValue>,
) -> Result<(
    RapidHashMap<String, JsonValue>,
    RapidHashMap<String, FieldOp>,
)> {
    let mut fields = RapidHashMap::with_capacity(input.len());
    let mut ops = RapidHashMap::new();
    for (key, value) in input {
        if !FIELD_OPS.contains(&key.as_str()) {
            fields.insert(key, value);
            continue;
        }
        let JsonValue::Object(targets) = value else {
            return Err(QueryError::parse(format!(
                "`{key}` takes an object keyed by field name; a field named `{key}` is written \
                 with `set: {{ {key}: ... }}`"
            )));
        };
        for (field, arg) in targets {
            let op = match key.as_str() {
                "set" => FieldOp::Set(arg),
                "increment" => FieldOp::Increment(arg),
                op => FieldOp::Text(text_ops(op, &field, &arg)?),
            };
            if ops.insert(field.clone(), op).is_some() {
                return Err(two_ops(&field));
            }
        }
    }
    if let Some(field) = ops.keys().find(|field| fields.contains_key(*field)) {
        return Err(two_ops(field));
    }
    Ok((fields, ops))
}

fn two_ops(field: &str) -> QueryError {
    QueryError::parse(format!(
        "`{field}` is given more than one op; send one op per field in an update"
    ))
}

fn text_ops(op: &str, field: &str, arg: &JsonValue) -> Result<Vec<TextOp>> {
    let usage = || {
        QueryError::parse(match op {
            "insert" => format!("`insert` on `{field}` takes {{ at: Int, value: String }}"),
            "delete" => format!("`delete` on `{field}` takes {{ at: Int, length: Int }}"),
            _ => format!("`replace` on `{field}` takes {{ at: Int, length: Int, value: String }}"),
        })
    };
    let number = |name| arg.get(name).and_then(JsonValue::as_u64).ok_or_else(usage);
    let text = || {
        arg.get("value")
            .and_then(JsonValue::as_str)
            .map(str::to_string)
            .ok_or_else(usage)
    };
    let pos = number("at")?;
    Ok(match op {
        "insert" => vec![TextOp::Insert { pos, text: text()? }],
        "delete" => vec![TextOp::Delete {
            pos,
            len: number("length")?,
        }],
        _ => vec![
            TextOp::Delete {
                pos,
                len: number("length")?,
            },
            TextOp::Insert { pos, text: text()? },
        ],
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn split(
        input: JsonValue,
    ) -> Result<(
        RapidHashMap<String, JsonValue>,
        RapidHashMap<String, FieldOp>,
    )> {
        let JsonValue::Object(input) = input else {
            unreachable!("tests pass objects")
        };
        split_update_input(input.into_iter().collect())
    }

    #[test]
    fn field_keys_stay_implicit_sets() {
        let (fields, ops) = split(json!({"name": "John", "age": 3})).unwrap();
        assert_eq!(fields.len(), 2);
        assert!(ops.is_empty());
    }

    #[test]
    fn ops_are_keyed_by_op_then_field() {
        let (fields, ops) = split(json!({
            "name": "John",
            "set": {"title": "Dr"},
            "increment": {"visits": 2},
            "insert": {"body": {"at": 4, "value": "quick "}},
            "replace": {"note": {"at": 0, "length": 2, "value": "x"}},
        }))
        .unwrap();
        assert_eq!(fields.keys().collect::<Vec<_>>(), ["name"]);
        assert_eq!(ops["title"], FieldOp::Set(json!("Dr")));
        assert_eq!(ops["visits"], FieldOp::Increment(json!(2)));
        assert_eq!(
            ops["body"],
            FieldOp::Text(vec![TextOp::Insert {
                pos: 4,
                text: "quick ".into()
            }])
        );
        assert_eq!(
            ops["note"],
            FieldOp::Text(vec![
                TextOp::Delete { pos: 0, len: 2 },
                TextOp::Insert {
                    pos: 0,
                    text: "x".into()
                }
            ])
        );
    }

    #[test]
    fn a_field_named_like_an_op_is_reached_through_set() {
        let (fields, ops) = split(json!({"set": {"set": "value", "delete": true}})).unwrap();
        assert!(fields.is_empty());
        assert_eq!(ops["set"], FieldOp::Set(json!("value")));
        assert_eq!(ops["delete"], FieldOp::Set(json!(true)));
    }

    #[test]
    fn one_field_takes_one_op() {
        assert!(split(json!({"body": "x", "insert": {"body": {"at": 0, "value": "y"}}})).is_err());
        assert!(split(json!({"set": {"n": 1}, "increment": {"n": 1}})).is_err());
    }

    #[test]
    fn malformed_ops_say_how_to_write_them() {
        let error = split(json!({"set": "value"})).unwrap_err().to_string();
        assert!(error.contains("set: { set: ... }"), "{error}");
        let error = split(json!({"delete": {"body": {"at": 1}}}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("length"), "{error}");
    }
}

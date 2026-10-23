//! Explicit CRDT ops in an update's input.
//!
//! `update_X(input: { insert: { body: { at: 4, value: "quick " } } })` names an
//! op, then the fields it applies to. Any other key is a field written with an
//! implicit `set`, so `input: { name: "John" }` keeps its meaning. A key that
//! names one of the collection's fields is always that field, as it is in Go,
//! so a collection with a field named like an op writes that field and cannot
//! use that op.

use defra_core::TextOp;
use rapidhash::{HashMapExt, RapidHashMap};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::error::{QueryError, Result};

/// The op names an update input reads as ops rather than fields.
pub const FIELD_OPS: [&str; 5] = ["set", "increment", "insert", "delete", "replace"];

/// One explicit op on a field, before its value is read as the field's kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InputOp {
    /// Write this value.
    Set(JsonValue),
    /// Add this amount.
    Increment(JsonValue),
    /// Edit the field's characters, in positions of its current text.
    Text(Vec<TextOp>),
}

/// Split an update input into its implicitly set fields and its explicit ops.
/// `is_field` says whether a key names one of the collection's fields; such a
/// key is never read as an op.
pub fn split_update_input<IsField>(
    input: &RapidHashMap<String, JsonValue>,
    is_field: IsField,
) -> Result<(
    RapidHashMap<String, JsonValue>,
    RapidHashMap<String, InputOp>,
)>
where
    IsField: Fn(&str) -> bool,
{
    let mut fields = RapidHashMap::with_capacity(input.len());
    let mut ops = RapidHashMap::new();
    for (key, value) in input {
        if !FIELD_OPS.contains(&key.as_str()) || is_field(key) {
            fields.insert(key.clone(), value.clone());
            continue;
        }
        let JsonValue::Object(targets) = value else {
            return Err(QueryError::parse(format!(
                "`{key}` takes an object keyed by field name"
            )));
        };
        for (field, arg) in targets {
            let op = match key.as_str() {
                "set" => InputOp::Set(arg.clone()),
                "increment" => InputOp::Increment(arg.clone()),
                op => InputOp::Text(text_ops(op, field, arg)?),
            };
            if ops.insert(field.clone(), op).is_some() {
                return Err(two_ops(field));
            }
        }
    }
    if let Some(field) = ops.keys().find(|field| fields.contains_key(*field)) {
        return Err(two_ops(field));
    }
    Ok((fields, ops))
}

pub(crate) fn two_ops(field: &str) -> QueryError {
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
        RapidHashMap<String, InputOp>,
    )> {
        split_with(input, &[])
    }

    fn split_with(
        input: JsonValue,
        schema_fields: &[&str],
    ) -> Result<(
        RapidHashMap<String, JsonValue>,
        RapidHashMap<String, InputOp>,
    )> {
        let JsonValue::Object(input) = input else {
            unreachable!("tests pass objects")
        };
        split_update_input(&input.into_iter().collect(), |key| {
            schema_fields.contains(&key)
        })
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
        assert_eq!(ops["title"], InputOp::Set(json!("Dr")));
        assert_eq!(ops["visits"], InputOp::Increment(json!(2)));
        assert_eq!(
            ops["body"],
            InputOp::Text(vec![TextOp::Insert {
                pos: 4,
                text: "quick ".into()
            }])
        );
        assert_eq!(
            ops["note"],
            InputOp::Text(vec![
                TextOp::Delete { pos: 0, len: 2 },
                TextOp::Insert {
                    pos: 0,
                    text: "x".into()
                }
            ])
        );
    }

    #[test]
    fn a_key_naming_a_field_is_that_field() {
        let (fields, ops) = split_with(
            json!({"set": {"name": "x"}, "delete": true, "increment": {"n": 1}}),
            &["set", "delete", "name", "n"],
        )
        .unwrap();
        assert_eq!(fields["set"], json!({"name": "x"}));
        assert_eq!(fields["delete"], json!(true));
        assert_eq!(ops["n"], InputOp::Increment(json!(1)));
        assert_eq!(fields.len(), 2);
        assert_eq!(ops.len(), 1);
    }

    #[test]
    fn one_field_takes_one_op() {
        assert!(split(json!({"body": "x", "insert": {"body": {"at": 0, "value": "y"}}})).is_err());
        assert!(split(json!({"set": {"n": 1}, "increment": {"n": 1}})).is_err());
    }

    #[test]
    fn malformed_ops_say_how_to_write_them() {
        let error = split(json!({"set": "value"})).unwrap_err().to_string();
        assert!(error.contains("keyed by field name"), "{error}");
        let error = split(json!({"delete": {"body": {"at": 1}}}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("length"), "{error}");
    }
}

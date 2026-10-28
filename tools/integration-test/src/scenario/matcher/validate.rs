use serde_json::{Map, Value};

use super::explain::{valid_explain_graph, valid_explain_scan};
use super::scalar::{expected_length, valid_similarity};
use crate::scenario::ScenarioError;

const EXPECTED: &str = "expected cid, docID, nonempty, nonemptyString, introspectionTypes, \
    explainScan metrics, explainPattern, explainTarget, named same/unique/branchableCreateGraph, \
    all/anyOf, optional value, firstWhere field/value/expect, object fields, unordered/contains \
    items, similarityScore vectors, length value, substring value, or sortedStrings field";

fn invalid() -> ScenarioError {
    ScenarioError::Matcher(EXPECTED.to_owned())
}

fn has_str(map: &Map<String, Value>, key: &str) -> bool {
    map.get(key).is_some_and(Value::is_string)
}

fn each_valid<'a, I>(children: I) -> Result<(), ScenarioError>
where
    I: IntoIterator<Item = &'a Value>,
{
    for child in children {
        validate(child)?;
    }
    Ok(())
}

/// Checks every matcher object in `value` and reports whether there is one.
pub fn validate(value: &Value) -> Result<bool, ScenarioError> {
    match value {
        Value::Object(map) => match map.get("$match") {
            Some(kind) => validate_matcher(kind, map).map(|()| true),
            None => {
                let mut found = false;
                for child in map.values() {
                    found |= validate(child)?;
                }
                Ok(found)
            }
        },
        Value::Array(items) => {
            let mut found = false;
            for child in items {
                found |= validate(child)?;
            }
            Ok(found)
        }
        _ => Ok(false),
    }
}

fn validate_matcher(kind: &Value, map: &Map<String, Value>) -> Result<(), ScenarioError> {
    let kind = kind.as_str().ok_or_else(invalid)?;
    let len = map.len();
    let ok = match kind {
        "explainPattern" | "explainTarget" => valid_explain_graph(map),
        "explainScan" => valid_explain_scan(map),
        "similarityScore" => valid_similarity(map),
        "notPlainCBORString" => {
            let value_ok = match map.get("value") {
                Some(Value::String(_)) => true,
                Some(Value::Object(reference)) if reference.len() == 1 => reference
                    .get("$ref")
                    .and_then(Value::as_str)
                    .is_some_and(|name| !name.is_empty()),
                _ => false,
            };
            len == 2 && value_ok
        }
        "length" => {
            len == 2
                && map
                    .get("value")
                    .is_some_and(|v| expected_length(v).is_some())
        }
        "substring" => len == 2 && has_str(map, "value"),
        "sortedStrings" => len == 2 && has_str(map, "field"),
        "nonempty" | "cid" | "docID" | "introspectionTypes" | "nonemptyString"
        | "currentTimestamp" => len == 1,
        "same" | "unique" | "branchableCreateGraph" => {
            len == 2
                && map
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| !name.trim().is_empty())
        }
        "optional" => match map.get("value") {
            Some(child) if len == 2 => {
                validate(child)?;
                true
            }
            _ => false,
        },
        "all" | "anyOf" => match map.get("of").and_then(Value::as_array) {
            Some(clauses) if len == 2 && !clauses.is_empty() => {
                each_valid(clauses)?;
                true
            }
            _ => false,
        },
        "firstWhere" => match map.get("expect") {
            Some(expected) if len == 4 && has_str(map, "field") && has_str(map, "value") => {
                validate(expected)?;
                true
            }
            _ => false,
        },
        "object" => match map.get("fields").and_then(Value::as_object) {
            Some(fields) if len == 2 && !fields.is_empty() => {
                each_valid(fields.values())?;
                true
            }
            _ => false,
        },
        "contains" => match map.get("items").and_then(Value::as_array) {
            Some(items) if len == 2 => {
                for item in items {
                    if validate(item)? {
                        return Err(ScenarioError::Matcher(
                            "contains items must be exact values, not matchers".to_owned(),
                        ));
                    }
                }
                true
            }
            _ => false,
        },
        "unordered" => match map.get("items").and_then(Value::as_array) {
            Some(items) if len == 2 => {
                each_valid(items)?;
                true
            }
            _ => false,
        },
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(invalid())
    }
}

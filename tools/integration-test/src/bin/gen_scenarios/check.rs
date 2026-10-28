//! The round trip: every generated test, exported, must equal its corpus
//! scenario after the documented normalizations.

use std::path::Path;

use integration_test::scenario::number::json_eq;
use serde_json::Value;

use super::corpus::Loaded;
use super::error::{io, GenError, Result};
use super::names;
use super::text::{dedent, DEDENTED};

fn normalize_args(args: &mut serde_json::Map<String, Value>) {
    if args.get("node").and_then(Value::as_str) == Some("db") {
        args.shift_remove("node");
    }
    for key in DEDENTED {
        if let Some(Value::String(text)) = args.get_mut(*key) {
            if text.contains('\n') {
                *text = dedent(text);
            }
        }
    }
    for child in args
        .get_mut("children")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(inner) = child.get_mut("args").and_then(Value::as_object_mut) {
            normalize_args(inner);
        }
    }
}

fn normalize_step(step: &mut Value) {
    if let Some(args) = step.get_mut("args").and_then(Value::as_object_mut) {
        normalize_args(args);
    }
}

/// Renames transactions by the order they begin, so a scenario that names
/// its first transaction `txn1` equals the same scenario with `txn0`: the
/// names are labels, and an action names a transaction by that order.
fn rename_transactions(steps: &mut [Value]) {
    let mut order: Vec<String> = Vec::new();
    for step in steps.iter() {
        if step["op"] == "transaction.begin" {
            if let Some(name) = step["args"]["transaction"].as_str() {
                order.push(name.to_owned());
            }
        }
    }
    fn rename(step: &mut Value, order: &[String]) {
        if let Some(args) = step.get_mut("args").and_then(Value::as_object_mut) {
            if let Some(name) = args.get("transaction").and_then(Value::as_str) {
                if let Some(index) = order.iter().position(|n| n == name) {
                    args.insert(
                        "transaction".to_owned(),
                        Value::String(format!("txn{index}")),
                    );
                }
            }
            for child in args
                .get_mut("children")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                rename(child, order);
            }
        }
    }
    for step in steps.iter_mut() {
        rename(step, &order);
    }
}

/// The comparable part of a scenario: steps, with the trailing run of
/// `node.close` ordered by node, and the skip reason.
fn normalize(scenario: &Value) -> Value {
    let mut steps = scenario["steps"].as_array().cloned().unwrap_or_default();
    steps.iter_mut().for_each(normalize_step);
    rename_transactions(&mut steps);
    let trailing = steps
        .iter()
        .rev()
        .take_while(|s| s["op"] == "node.close")
        .count();
    let start = steps.len() - trailing;
    steps[start..].sort_by_key(|s| s["args"]["node"].as_str().unwrap_or_default().to_owned());
    let mut out = serde_json::Map::new();
    out.insert("steps".to_owned(), Value::Array(steps));
    if let Some(skip) = scenario.get("upstreamSkip") {
        out.insert("upstreamSkip".to_owned(), skip.clone());
    }
    Value::Object(out)
}

pub struct Outcome {
    pub checked: usize,
    pub mismatched: Vec<String>,
    pub missing: Vec<String>,
}

pub fn run(loaded: &[Loaded], exported: &Path, originals: &Path) -> Result<Outcome> {
    let mut outcome = Outcome {
        checked: 0,
        mismatched: Vec::new(),
        missing: Vec::new(),
    };
    for item in loaded {
        let id = names::export_id(item);
        let path = exported.join(format!("{id}.json"));
        if !path.exists() {
            outcome.missing.push(id);
            continue;
        }
        let read = |p: &Path| -> Result<Value> {
            let text = io(p, std::fs::read_to_string(p))?;
            serde_json::from_str(&text).map_err(|source| GenError::Json {
                path: p.to_owned(),
                source,
            })
        };
        let mut original_path = originals.to_path_buf();
        original_path.extend(&item.dir);
        original_path.push(format!("{}.json", item.stem));
        let original = normalize(&read(&original_path)?);
        let rebuilt = normalize(&read(&path)?);
        outcome.checked += 1;
        if !json_eq(&original, &rebuilt) {
            outcome.mismatched.push(id);
        }
    }
    Ok(outcome)
}

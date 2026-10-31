//! Writes a test case back out as a scenario file, so the Rust suite is the
//! source and the JSON corpus an export of it.

use std::path::Path;

use serde_json::{json, Map, Value};

use super::unit_ops::is_unit_op;
use super::wire::WireStep;

const DB: &str = "db";
const OPEN_PREFIX: &str = "node.open";

const ARG_REQUIREMENTS: &[(&str, &str)] = &[
    ("identitySeed", "request.seedIdentity"),
    ("targetIdentitySeed", "request.seedIdentity"),
    ("identityNode", "request.nodeIdentity"),
    ("targetIdentityNode", "request.nodeIdentity"),
    ("variables", "query.variables"),
    ("operationName", "query.operationName"),
    ("nonFiniteFloats", "query.nonFiniteFloats"),
    ("resultTypes", "query.resultTypes"),
];

fn node_of(step: &WireStep) -> &str {
    step.args.get("node").and_then(Value::as_str).unwrap_or(DB)
}

fn lifecycle(op: &str, node: &str) -> Value {
    json!({"op": op, "args": {"node": node}, "expect": {}})
}

/// The scenario for `steps`: the implicit `db` node opened first and every
/// node still open closed last, the way the corpus spells them.
pub fn scenario(id: &str, steps: &[WireStep], without_db: bool, skip: Option<&str>) -> Value {
    let mut named = steps.to_vec();
    for (began, step) in named
        .iter_mut()
        .filter(|s| s.op == "transaction.begin")
        .enumerate()
    {
        if let Some(args) = step.args.as_object_mut() {
            args.insert(
                "transaction".to_owned(),
                json!(super::txn_name(began as u64)),
            );
        }
    }
    let steps = named.as_slice();
    let mut opened: Vec<String> = Vec::new();
    for step in steps.iter().filter(|s| s.op.starts_with(OPEN_PREFIX)) {
        if !opened.iter().any(|n| n == node_of(step)) {
            opened.push(node_of(step).to_owned());
        }
    }
    let implicit = !without_db && !opened.iter().any(|n| n == DB);
    if implicit {
        opened.insert(0, DB.to_owned());
    }
    let mut out: Vec<Value> = Vec::new();
    if implicit {
        out.push(lifecycle("node.open", DB));
    }
    let mut index = 0;
    while index < steps.len() {
        if !steps[index].envelope {
            out.push(step_json(&steps[index]));
            index += 1;
            continue;
        }
        let end = steps[index..]
            .iter()
            .position(|s| !s.envelope)
            .map_or(steps.len(), |n| index + n);
        out.push(group_json(&steps[index..end]));
        index = end;
    }
    if !steps.last().is_some_and(|s| s.op == "node.close") {
        for node in &opened {
            out.push(lifecycle("node.close", node));
        }
    }
    let mut requires: Vec<String> = Vec::new();
    let mut require = |name: &str| {
        if !requires.iter().any(|r| r == name) {
            requires.push(name.to_owned());
        }
    };
    for step in &out {
        require(step["op"].as_str().unwrap_or_default());
        for child in step["args"]["children"].as_array().into_iter().flatten() {
            require(child["op"].as_str().unwrap_or_default());
        }
        for (arg, requirement) in ARG_REQUIREMENTS {
            if step["args"].get(*arg).is_some() {
                require(requirement);
            }
        }
    }
    let mut result = Map::new();
    result.insert("id".to_owned(), json!(id));
    result.insert("version".to_owned(), json!(1));
    result.insert("requires".to_owned(), json!(requires));
    result.insert("steps".to_owned(), Value::Array(out));
    if let Some(reason) = skip {
        result.insert("upstreamSkip".to_owned(), json!(reason));
    }
    Value::Object(result)
}

fn group_json(children: &[WireStep]) -> Value {
    const SAFE: &[&str] = &[
        "document.createWithRetry",
        "query.execute",
        "collection.truncate",
    ];
    let wire: Vec<String> = children.iter().map(WireStep::wire_op).collect();
    let action = wire.iter().all(|op| SAFE.contains(&op.as_str()))
        && children.iter().all(|c| c.args.get("transaction").is_none());
    let kids: Vec<Value> = children
        .iter()
        .map(|child| {
            let mut args = child.args.clone();
            if let Some(map) = args.as_object_mut() {
                map.entry("node").or_insert_with(|| json!(DB));
            }
            json!({"op": child.wire_op(), "args": args})
        })
        .collect();
    let results: Vec<Value> = children
        .iter()
        .map(|c| c.check.expect.clone().unwrap_or(Value::Null))
        .collect();
    json!({
        "op": if action { "action.parallel" } else { "document.parallel" },
        "args": {"node": node_of(&children[0]), "children": kids},
        "expect": {"results": results},
    })
}

fn step_json(step: &WireStep) -> Value {
    let mut value = serde_json::to_value(step).unwrap_or(Value::Null);
    if let Some(args) = value.get_mut("args").and_then(Value::as_object_mut) {
        args.entry("node").or_insert_with(|| json!(DB));
    }
    let unit = is_unit_op(&step.wire_op()) && step.check.expect_error.is_none();
    if unit && step.check.expect.is_none() {
        value["expect"] = json!({});
    }
    value
}

pub fn write(dir: &Path, id: &str, scenario: &Value) {
    std::fs::create_dir_all(dir).expect("create export dir");
    let text = serde_json::to_string_pretty(scenario).expect("encode scenario");
    std::fs::write(dir.join(format!("{id}.json")), text).expect("write scenario");
}

/// Checks an exported scenario the way the corpus runner validates one, and
/// that importing it and exporting it again gives it back.
pub fn check(scenario: &Value, without_db: bool) -> Result<(), String> {
    let requires: Vec<&str> = scenario["requires"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let steps: Vec<WireStep> = serde_json::from_value(scenario["steps"].clone())
        .map_err(|e| format!("steps do not parse: {e}"))?;
    for step in &steps {
        let op = step.wire_op();
        if !requires.contains(&op.as_str()) {
            return Err(format!("{op} is not in requires"));
        }
        let check = &step.check;
        if check.eventually.is_some()
            && (op != "query.execute" || check.expect.is_none() || check.expect_error.is_some())
        {
            return Err(format!("{op}: eventually needs a query with expect"));
        }
        if let Some(message) = &check.expect_error {
            let exclusive = check.expect.is_none()
                && check.capture.is_empty()
                && check.compare.is_none()
                && check.unordered.is_empty()
                && check.unordered_documents.is_empty();
            if message.trim().is_empty() || !exclusive {
                return Err(format!("{op}: expectError must stand alone"));
            }
        }
    }
    let id = scenario["id"].as_str().unwrap_or_default();
    let skip = scenario["upstreamSkip"].as_str();
    let again = self::scenario(id, &steps, without_db, skip);
    match crate::scenario::number::json_eq(&again, scenario) {
        true => Ok(()),
        false => Err("export does not survive an import".to_owned()),
    }
}

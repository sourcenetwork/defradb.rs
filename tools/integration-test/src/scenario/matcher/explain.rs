use serde_json::{Map, Value};

use crate::scenario::number::json_eq;

use super::contains_matcher;

const PLAN_NODES: &[&str] = &[
    "explain",
    "root",
    "subType",
    "averageNode",
    "countNode",
    "addNode",
    "dagScanNode",
    "deleteNode",
    "groupNode",
    "limitNode",
    "maxNode",
    "minNode",
    "multiScanNode",
    "orderNode",
    "orphanNode",
    "parallelNode",
    "sequenceNode",
    "pipeNode",
    "scanNode",
    "selectNode",
    "selectTopNode",
    "sumNode",
    "topLevelNode",
    "typeIndexJoin",
    "typeJoinMany",
    "typeJoinOne",
    "updateNode",
    "upsertNode",
    "valuesNode",
    "viewNode",
    "lensNode",
    "operationNode",
    "similarityNode",
];

const SCAN_METRICS: &[&str] = &[
    "iterations",
    "docFetches",
    "fieldFetches",
    "indexFetches",
    "filterMatches",
    "sizeOfResult",
    "planExecutions",
];

fn is_plan_node(key: &str) -> bool {
    PLAN_NODES.contains(&key)
}

pub fn valid_explain_scan(map: &Map<String, Value>) -> bool {
    let Some(metrics) = map.get("metrics").and_then(Value::as_object) else {
        return false;
    };
    let path_ok = match map.get("path") {
        Some(path) => {
            map.len() == 3
                && path
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string))
        }
        None => map.len() == 2,
    };
    path_ok
        && metrics.iter().all(|(key, value)| {
            SCAN_METRICS.contains(&key.as_str())
                && (value.as_u64().is_some() || metric_floor(value).is_some())
        })
}

/// The least count a metric matcher accepts: `positive` is at least one,
/// `atLeast` names its floor. `None` when `expected` is a plain count.
fn metric_floor(expected: &Value) -> Option<u64> {
    let matcher = expected.as_object()?;
    match (matcher.get("$match")?.as_str()?, matcher.len()) {
        ("positive", 1) => Some(1),
        ("atLeast", 2) => matcher.get("value")?.as_u64(),
        _ => None,
    }
}

pub fn valid_explain_graph(map: &Map<String, Value>) -> bool {
    let literal = if map.get("$match").and_then(Value::as_str) == Some("explainPattern") {
        match (map.len(), map.get("pattern")) {
            (2, Some(pattern @ Value::Object(_))) => pattern,
            _ => return false,
        }
    } else {
        let name_ok = map
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| !name.is_empty());
        let children_ok = map.get("includeChildren").is_some_and(Value::is_boolean);
        let skip_ok = map.get("skip").is_some_and(|skip| skip.as_u64().is_some());
        match map.get("attributes") {
            Some(attributes) if map.len() == 5 && name_ok && children_ok && skip_ok => attributes,
            _ => return false,
        }
    };
    !contains_matcher(literal)
}

fn trim_pattern(value: &Value) -> Option<Value> {
    let Value::Object(map) = value else {
        return Some(Value::Null);
    };
    let mut result = Map::new();
    for (key, child) in map {
        if !is_plan_node(key) {
            continue;
        }
        let trimmed = match child {
            Value::Object(_) => trim_pattern(child)?,
            Value::Array(items) => {
                Value::Array(items.iter().map(trim_pattern).collect::<Option<Vec<_>>>()?)
            }
            _ => return None,
        };
        result.insert(key.clone(), trimmed);
    }
    Some(Value::Object(result))
}

fn find_target<'a>(value: &'a Value, name: &str, skip: &mut u64) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                if !is_plan_node(key) {
                    continue;
                }
                if key == name {
                    if *skip == 0 {
                        return Some(&map[key]);
                    }
                    *skip -= 1;
                }
                if let Some(found) = find_target(&map[key], name, skip) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|item| find_target(item, name, skip)),
        _ => None,
    }
}

pub fn match_explain_graph(want: &Map<String, Value>, actual: &Value) -> bool {
    let Value::Object(data) = actual else {
        return false;
    };
    if want.get("$match").and_then(Value::as_str) == Some("explainPattern") {
        return trim_pattern(actual).is_some_and(|trimmed| json_eq(&want["pattern"], &trimmed));
    }
    let name = want["name"].as_str().unwrap_or_default();
    let include_children = want["includeChildren"].as_bool().unwrap_or_default();
    for root in data.values() {
        let mut skip = want["skip"].as_u64().unwrap_or_default();
        let Some(found) = find_target(root, name, &mut skip) else {
            return false;
        };
        let found = match (include_children, found) {
            (false, Value::Object(map)) => Value::Object(
                map.iter()
                    .filter(|(key, _)| !is_plan_node(key))
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            ),
            _ => found.clone(),
        };
        if !json_eq(&want["attributes"], &found) {
            return false;
        }
    }
    true
}

fn object<'a>(map: &'a Map<String, Value>, key: &str) -> Option<&'a Map<String, Value>> {
    map.get(key)?.as_object()
}

fn unwrap_nodes<'a>(mut node: &'a Map<String, Value>, keys: &[&str]) -> &'a Map<String, Value> {
    for key in keys {
        if let Some(child) = object(node, key) {
            node = child;
        }
    }
    node
}

fn first_orphan(join: &Map<String, Value>) -> Option<&Map<String, Value>> {
    if let Some(orphan) = object(join, "orphanNode") {
        return Some(orphan);
    }
    join.get("sequenceNode")?
        .as_array()?
        .iter()
        .filter_map(Value::as_object)
        .find_map(|item| object(item, "orphanNode"))
}

fn metrics_at<'a>(
    select: &'a Map<String, Value>,
    path: &[Value],
) -> Option<&'a Map<String, Value>> {
    if let Some(scan) = object(select, "scanNode") {
        return path.is_empty().then_some(scan);
    }
    let mut join = object(select, "typeIndexJoin")?;
    let first = path.first()?;
    if first.as_str() == Some("orphanNode") {
        return first_orphan(join);
    }
    if let Some(sequence) = join.get("sequenceNode").and_then(Value::as_array) {
        let non_orphan = sequence
            .iter()
            .filter_map(Value::as_object)
            .find(|item| !item.contains_key("orphanNode"));
        if let Some(item) = non_orphan {
            join = item;
        }
    }
    if let Some(orphan) = object(join, "orphanNode") {
        join = orphan;
    }
    let mut current = join;
    for element in path {
        let level = object(current, "typeJoinMany")
            .or_else(|| object(current, "typeJoinOne"))
            .unwrap_or(current);
        current = match element.as_str()? {
            "root" => object(level, "root")?,
            "subType" => {
                let sub = object(level, "subType")?;
                let sub = unwrap_nodes(
                    sub,
                    &[
                        "selectTopNode",
                        "limitNode",
                        "orderNode",
                        "similarityNode",
                        "selectNode",
                    ],
                );
                object(sub, "typeIndexJoin").unwrap_or(sub)
            }
            other => object(current, other)?,
        };
    }
    if let Some(scan) = object(current, "scanNode") {
        return Some(scan);
    }
    ["typeJoinMany", "typeJoinOne"]
        .iter()
        .find_map(|key| object(object(object(current, key)?, "root")?, "scanNode"))
}

/// A counter the plan leaves out counts as zero, as it did before floors; a
/// counter that is there must be an unsigned count.
fn count_matches(value: Option<&Value>, expected: &Value) -> bool {
    let count = match value {
        None => 0,
        Some(value) => match value.as_u64() {
            Some(count) => count,
            None => return false,
        },
    };
    match metric_floor(expected) {
        Some(floor) => count >= floor,
        None => expected.as_u64() == Some(count),
    }
}

/// Nodes that wrap a request's `selectNode`, in whatever order each runtime
/// nests them (Go puts `sumNode` above `countNode`, Rust the other way round).
const WRAPPER_NODES: [&str; 9] = [
    "limitNode",
    "orderNode",
    "similarityNode",
    "groupNode",
    "countNode",
    "sumNode",
    "averageNode",
    "minNode",
    "maxNode",
];

pub fn match_explain_scan(want: &Map<String, Value>, actual: &Value) -> bool {
    let Some(explain) = actual.get("explain").and_then(Value::as_object) else {
        return false;
    };
    if explain.get("executionSuccess") != Some(&Value::Bool(true)) {
        return false;
    }
    let Some([operation]) = explain
        .get("operationNode")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
    else {
        return false;
    };
    let Some(mut node) = operation.get("selectTopNode").and_then(Value::as_object) else {
        return false;
    };
    while let Some(child) = WRAPPER_NODES.iter().find_map(|key| object(node, key)) {
        node = child;
    }
    let Some(select) = object(node, "selectNode") else {
        return false;
    };
    let path: &[Value] = want
        .get("path")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice);
    let Some(scan) = metrics_at(select, path) else {
        return false;
    };
    let Some(metrics) = want["metrics"].as_object() else {
        return false;
    };
    for (key, expected) in metrics {
        let value = match key.as_str() {
            "sizeOfResult" | "planExecutions" => explain.get(key),
            "filterMatches" => match select.get(key) {
                Some(value) => Some(value),
                None => return false,
            },
            _ => scan.get(key),
        };
        if !count_matches(value, expected) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::count_matches;

    #[test]
    fn count_matcher_requires_an_unsigned_count() {
        assert!(count_matches(Some(&json!(0)), &json!(0)));
        assert!(count_matches(
            Some(&json!(2)),
            &json!({"$match": "atLeast", "value": 2})
        ));
        assert!(count_matches(
            Some(&json!(1)),
            &json!({"$match": "positive"})
        ));
        assert!(count_matches(None, &json!(0)));
        assert!(!count_matches(None, &json!({"$match": "positive"})));
        assert!(!count_matches(Some(&json!(-1)), &json!(0)));
        assert!(!count_matches(
            Some(&json!(-1)),
            &json!({"$match": "positive"})
        ));
    }
}

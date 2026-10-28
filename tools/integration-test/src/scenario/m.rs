//! Constructors for the reserved expectation objects, so a scenario reads as
//! `m::object(json!({..}))` instead of `{"$match": "object", ..}`.

use serde_json::{json, Value};

pub fn object(fields: Value) -> Value {
    json!({"$match": "object", "fields": fields})
}

pub fn optional(value: Value) -> Value {
    json!({"$match": "optional", "value": value})
}

pub fn unordered(items: Value) -> Value {
    json!({"$match": "unordered", "items": items})
}

pub fn contains(items: Value) -> Value {
    json!({"$match": "contains", "items": items})
}

pub fn all(of: Value) -> Value {
    json!({"$match": "all", "of": of})
}

pub fn any_of(of: Value) -> Value {
    json!({"$match": "anyOf", "of": of})
}

pub fn same(name: &str) -> Value {
    json!({"$match": "same", "name": name})
}

pub fn unique(name: &str) -> Value {
    json!({"$match": "unique", "name": name})
}

pub fn cid() -> Value {
    json!({"$match": "cid"})
}

pub fn doc_id() -> Value {
    json!({"$match": "docID"})
}

pub fn nonempty() -> Value {
    json!({"$match": "nonempty"})
}

pub fn nonempty_string() -> Value {
    json!({"$match": "nonemptyString"})
}

pub fn length(count: u64) -> Value {
    json!({"$match": "length", "value": count})
}

pub fn substring(text: &str) -> Value {
    json!({"$match": "substring", "value": text})
}

pub fn sorted_strings(field: &str) -> Value {
    json!({"$match": "sortedStrings", "field": field})
}

pub fn first_where(field: &str, value: &str, expect: Value) -> Value {
    json!({"$match": "firstWhere", "field": field, "value": value, "expect": expect})
}

pub fn similarity_score(metric: &str, source: Value, vector: Value) -> Value {
    json!({"$match": "similarityScore", "metric": metric, "source": source, "vector": vector})
}

pub fn current_timestamp() -> Value {
    json!({"$match": "currentTimestamp"})
}

pub fn not_plain_cbor_string(value: Value) -> Value {
    json!({"$match": "notPlainCBORString", "value": value})
}

pub fn introspection_types() -> Value {
    json!({"$match": "introspectionTypes"})
}

pub fn explain_scan(metrics: Value) -> Value {
    json!({"$match": "explainScan", "metrics": metrics})
}

pub fn explain_scan_at(metrics: Value, path: Value) -> Value {
    json!({"$match": "explainScan", "metrics": metrics, "path": path})
}

pub fn explain_pattern(pattern: Value) -> Value {
    json!({"$match": "explainPattern", "pattern": pattern})
}

pub fn explain_target(name: &str, include_children: bool, skip: u64, attributes: Value) -> Value {
    json!({
        "$match": "explainTarget",
        "name": name,
        "includeChildren": include_children,
        "skip": skip,
        "attributes": attributes,
    })
}

pub fn branchable_create_graph(name: &str) -> Value {
    json!({"$match": "branchableCreateGraph", "name": name})
}

pub fn r(name: &str) -> Value {
    json!({"$ref": name})
}

pub fn concat<I>(parts: I) -> Value
where
    I: IntoIterator,
    I::Item: Into<Value>,
{
    let parts: Vec<Value> = parts.into_iter().map(Into::into).collect();
    json!({"$concat": parts})
}

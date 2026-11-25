//! Type join nodes for resolving relations
//!
//! TypeJoinOne and TypeJoinMany implement the join logic for one-to-one
//! and one-to-many relations respectively. These nodes wrap a parent plan
//! and perform lookups to resolve related documents.

mod direction;
mod join_side;
mod metrics;
mod type_join_many;
mod type_join_one;

pub use direction::JoinDirection;
pub use join_side::JoinSide;
pub use metrics::JoinChildMetrics;
pub use type_join_many::{compare_json_values, resolve_nested_field, TypeJoinMany};
pub use type_join_one::{RelationFilter, TypeJoinOne};

/// Keep the name of the index that served a scan when replacing its metrics.
fn scan_node_with_index_name(
    scan_node: Option<&serde_json::Value>,
    metrics: serde_json::Value,
    lookup_index: Option<&str>,
) -> serde_json::Value {
    let mut result = metrics.as_object().cloned().unwrap_or_default();
    let name = lookup_index.or_else(|| {
        scan_node
            .and_then(|value| value.get("indexName"))
            .and_then(|value| value.as_str())
    });
    if let Some(name) = name {
        result.insert(
            "indexName".to_owned(),
            serde_json::Value::String(name.to_owned()),
        );
    }
    serde_json::Value::Object(result)
}

#[cfg(test)]
mod explain_tests {
    use super::scan_node_with_index_name;
    use serde_json::json;

    #[test]
    fn direct_lookup_index_takes_precedence_over_child_plan_index() {
        let plan = json!({"indexName": "child_index"});
        assert_eq!(
            scan_node_with_index_name(Some(&plan), json!({"indexFetches": 2}), Some("fk_index")),
            json!({"indexFetches": 2, "indexName": "fk_index"})
        );
        assert_eq!(
            scan_node_with_index_name(Some(&plan), json!({"indexFetches": 2}), None),
            json!({"indexFetches": 2, "indexName": "child_index"})
        );
    }
}

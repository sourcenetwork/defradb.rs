//! When a relation aggregate may read the rows of a join an earlier aggregate built.
//!
//! An aggregate join's child scan carries its aggregate's filter, and its rows are
//! rendered once, with the fields that aggregate asked for. A later aggregate can
//! read those rows only when its own scan filter is the same and every field it
//! reads was rendered. Go keys its hidden aggregate selections the same way
//! (`Targetable.equal`: name, filter, limit and order; limit and order are applied
//! per aggregate after the rows are read).

use rapidhash::RapidHashSet;

use crate::document::DocumentMapping;
use crate::mapper::{AggregateTarget, AggregateType, Filter};

/// One join built for a relation aggregate.
pub(super) struct AggregateJoin {
    pub(super) relation: String,
    scan_filter: Option<String>,
    rendered: RapidHashSet<String>,
    /// The private key its rows render under, or `None` for the relation's own key.
    pub(super) internal_key: Option<String>,
}

impl AggregateJoin {
    pub(super) fn new(
        relation: &str,
        scan_filter: Option<&Filter>,
        child_mapping: &DocumentMapping,
        internal_key: Option<String>,
    ) -> Self {
        Self {
            relation: relation.to_string(),
            scan_filter: scan_filter.map(filter_key),
            rendered: child_mapping
                .render_keys
                .iter()
                .map(|rk| rk.key.clone())
                .collect(),
            internal_key,
        }
    }

    /// Whether `target`, scanning with `scan_filter`, can read this join's rows.
    pub(super) fn serves(&self, target: &AggregateTarget, scan_filter: Option<&Filter>) -> bool {
        self.relation == target.host_name
            && target.group_by.is_none()
            && self.scan_filter == scan_filter.map(filter_key)
            && read_fields(target)
                .is_some_and(|fields| fields.iter().all(|field| self.rendered.contains(field)))
    }
}

/// The filter an aggregate's child scan applies: its target filter, plus
/// `{field: {_neq: null}}` for an average so null values are not counted.
pub(super) fn scan_filter(
    aggregate_type: AggregateType,
    target: &AggregateTarget,
) -> Option<Filter> {
    let field_name = match (aggregate_type, target.field_name.as_ref()) {
        (AggregateType::Average, Some(field_name)) => field_name,
        _ => return target.filter.clone(),
    };
    let neq_null = serde_json::json!({"_neq": serde_json::Value::Null});
    Some(match target.filter.as_ref() {
        Some(existing) => {
            let mut merged = existing.conditions().clone();
            merged
                .entry(field_name.clone())
                .and_modify(|v| {
                    if let serde_json::Value::Object(ref mut ops) = v {
                        ops.insert("_neq".to_string(), serde_json::Value::Null);
                    }
                })
                .or_insert(neq_null);
            Filter::from_conditions_with_max_depth(merged, existing.max_depth())
        }
        None => {
            let mut conditions = serde_json::Map::new();
            conditions.insert(field_name.clone(), neq_null);
            Filter::from_conditions(conditions)
        }
    })
}

fn filter_key(filter: &Filter) -> String {
    serde_json::to_string(filter.conditions()).unwrap_or_default()
}

/// The child fields `target` reads after the scan, or `None` when it orders by
/// a related field, whose sub-join only its own join builds.
fn read_fields(target: &AggregateTarget) -> Option<Vec<String>> {
    let mut fields: Vec<String> = target.field_name.iter().cloned().collect();
    if let Some(filter) = target.filter.as_ref() {
        fields.extend(filter.referenced_fields());
    }
    if let Some(order) = target.order.as_ref() {
        for condition in &order.conditions {
            if condition.fields.len() > 1 {
                return None;
            }
            fields.extend(condition.fields.first().cloned());
        }
    }
    fields.retain(|field| !field.is_empty() && !field.starts_with('_'));
    Some(fields)
}

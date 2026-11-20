//! Secondary relation ID condition extraction.

use serde_json::{Map, Value};

use super::filter_impl::Filter;

impl Filter {
    /// Remove secondary relation ID conditions from this filter.
    ///
    /// The planner evaluates those conditions after the join that computes the
    /// ID, so the scan and post-join selection must not apply them twice.
    pub(crate) fn without_secondary_relation_ids(
        &self,
        collection: &schema::CollectionVersion,
    ) -> Filter {
        let conditions = strip_secondary_relation_ids(self.conditions(), collection);
        Filter::from_conditions_with_max_depth(conditions, self.max_depth())
    }
}

fn strip_secondary_relation_ids(
    conditions: &Map<String, Value>,
    collection: &schema::CollectionVersion,
) -> Map<String, Value> {
    let mut stripped = Map::new();
    for (key, value) in conditions {
        if let Some(relation_name) = key
            .strip_prefix('_')
            .and_then(|name| name.strip_suffix("ID"))
        {
            let is_secondary = collection
                .field_by_name(relation_name)
                .is_some_and(|field| field.kind.is_relation() && !field.is_primary);
            if is_secondary {
                continue;
            }
        }

        let value = match (key.as_str(), value) {
            ("_and" | "_or", Value::Array(branches)) => Value::Array(
                branches
                    .iter()
                    .map(|branch| match branch.as_object() {
                        Some(conditions) => {
                            Value::Object(strip_secondary_relation_ids(conditions, collection))
                        }
                        None => branch.clone(),
                    })
                    .collect(),
            ),
            ("_not", Value::Object(conditions)) => {
                Value::Object(strip_secondary_relation_ids(conditions, collection))
            }
            (_, value) => value.clone(),
        };
        stripped.insert(key.clone(), value);
    }
    stripped
}

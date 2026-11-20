//! Secondary relation ID condition extraction.

use serde_json::Map;

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
        let mut conditions = Map::new();
        for (key, value) in self.conditions() {
            if let Some(relation_name) = key
                .strip_prefix('_')
                .and_then(|name| name.strip_suffix("ID"))
            {
                if let Some(relation_field) = collection.field_by_name(relation_name) {
                    if relation_field.kind.is_relation() && !relation_field.is_primary {
                        continue;
                    }
                }
            }
            conditions.insert(key.clone(), value.clone());
        }
        Filter::from_conditions_with_max_depth(conditions, self.max_depth())
    }
}

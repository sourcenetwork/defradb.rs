//! Secondary relation ID condition extraction.

use serde_json::{Map, Value};

use super::filter_impl::Filter;

/// A filter with its secondary relation ID conditions taken out.
pub(crate) struct WithoutSecondaryRelationIds {
    /// The conditions the scan may apply before any join.
    pub(crate) filter: Filter,
    /// Whether a logical operator names a secondary relation ID, so the whole
    /// filter must be evaluated after the join that computes the ID.
    pub(crate) nested: bool,
}

impl Filter {
    /// Remove secondary relation ID conditions from this filter.
    ///
    /// The planner evaluates those conditions after the join that computes the
    /// ID. A top-level ID condition is dropped; a top-level logical operator
    /// that names one anywhere is dropped whole, because removing a branch of an
    /// `_or` or `_not` would narrow the scan instead of widening it.
    pub(crate) fn without_secondary_relation_ids(
        &self,
        collection: &schema::CollectionVersion,
    ) -> WithoutSecondaryRelationIds {
        let mut nested = false;
        let conditions = self
            .conditions()
            .iter()
            .filter_map(|(key, value)| {
                if is_secondary_relation_id(key, collection) {
                    return None;
                }
                if key == "_and" && names_secondary_relation_id(value, collection) {
                    if let Some(branches) = value.as_array() {
                        let branches = branches
                            .iter()
                            .cloned()
                            .filter_map(|branch| match branch {
                                Value::Object(branch) => {
                                    let branch = without_nested_secondary_relation_ids(
                                        &branch,
                                        collection,
                                        &mut nested,
                                    );
                                    (!branch.is_empty()).then_some(Value::Object(branch))
                                }
                                branch => Some(branch),
                            })
                            .collect::<Vec<_>>();
                        if !branches.is_empty() {
                            return Some((key.clone(), Value::Array(branches)));
                        }
                    }
                    nested = true;
                    return None;
                }
                if is_logical(key) && names_secondary_relation_id(value, collection) {
                    nested = true;
                    return None;
                }
                Some((key.clone(), value.clone()))
            })
            .collect::<Map<_, _>>();
        WithoutSecondaryRelationIds {
            filter: Filter::from_conditions_with_max_depth(conditions, self.max_depth()),
            nested,
        }
    }
}

/// Strip ID conditions from one conjunct of an `_and`. Other conjuncts in the
/// same branch stay in the scan predicate; only an `_or` or `_not` is dropped
/// whole because removing part of it would narrow the scan.
fn without_nested_secondary_relation_ids(
    conditions: &Map<String, Value>,
    collection: &schema::CollectionVersion,
    nested: &mut bool,
) -> Map<String, Value> {
    conditions
        .iter()
        .filter_map(|(key, value)| {
            if is_secondary_relation_id(key, collection) {
                *nested = true;
                return None;
            }
            if key == "_and" && names_secondary_relation_id(value, collection) {
                if let Some(branches) = value.as_array() {
                    let branches = branches
                        .iter()
                        .cloned()
                        .filter_map(|branch| match branch {
                            Value::Object(branch) => {
                                let branch = without_nested_secondary_relation_ids(
                                    &branch, collection, nested,
                                );
                                (!branch.is_empty()).then_some(Value::Object(branch))
                            }
                            branch => Some(branch),
                        })
                        .collect::<Vec<_>>();
                    if !branches.is_empty() {
                        return Some((key.clone(), Value::Array(branches)));
                    }
                }
                *nested = true;
                return None;
            }
            if is_logical(key) && names_secondary_relation_id(value, collection) {
                *nested = true;
                return None;
            }
            Some((key.clone(), value.clone()))
        })
        .collect()
}

fn is_logical(key: &str) -> bool {
    matches!(key, "_and" | "_or" | "_not")
}

fn is_secondary_relation_id(key: &str, collection: &schema::CollectionVersion) -> bool {
    key.strip_prefix('_')
        .and_then(|name| name.strip_suffix("ID"))
        .and_then(|relation_name| collection.field_by_name(relation_name))
        .is_some_and(|field| field.kind.is_relation() && !field.is_primary)
}

fn names_secondary_relation_id(value: &Value, collection: &schema::CollectionVersion) -> bool {
    match value {
        Value::Array(branches) => branches
            .iter()
            .any(|branch| names_secondary_relation_id(branch, collection)),
        Value::Object(conditions) => conditions.iter().any(|(key, value)| {
            is_secondary_relation_id(key, collection)
                || (is_logical(key) && names_secondary_relation_id(value, collection))
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn books() -> schema::CollectionVersion {
        schema::CollectionVersion::new(
            "Book",
            "v1",
            "coll-book",
            vec![
                schema::FieldDescription::new(
                    "1",
                    "_docID",
                    schema::FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                schema::FieldDescription::new(
                    "2",
                    "title",
                    schema::FieldKind::string(),
                    schema::CType::LwwRegister,
                ),
                schema::FieldDescription::new(
                    "3",
                    "publisher",
                    schema::FieldKind::relation("Publisher", false),
                    schema::CType::LwwRegister,
                )
                .with_relation_name("book_publisher"),
                schema::FieldDescription::new(
                    "4",
                    "_publisherID",
                    schema::FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                )
                .with_relation_name("book_publisher"),
            ],
        )
    }

    #[test]
    fn an_and_keeps_its_id_free_conjunct_at_the_scan() {
        let filter = Filter::from_conditions(
            json!({"_and": [
                {"title": {"_eq": "X"}},
                {"_publisherID": {"_eq": "publisher"}}
            ]})
            .as_object()
            .unwrap()
            .clone(),
        );

        let stripped = filter.without_secondary_relation_ids(&books());
        assert!(stripped.nested);
        assert_eq!(
            Value::Object(stripped.filter.conditions().clone()),
            json!({"_and": [{"title": {"_eq": "X"}}]})
        );
    }
}

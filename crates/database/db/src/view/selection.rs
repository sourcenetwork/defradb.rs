//! The selection a materialized view's cache is built from.
//!
//! A view's items are what its query returns after its transform, so the
//! cache selects every field the view declares, Go's maximal select, not only
//! the fields of the source query: a field a lens adds is one the source never
//! names. A relation field keeps the inner selection its source query gives it.

use rapidhash::{HashMapExt, RapidHashMap};
use schema::CollectionVersion;
use serde_json::Value;

use crate::error::{Error, Result};

/// The field list of the query that fills `view`'s cache, from its stored
/// source `query` (a Select JSON).
pub(crate) fn cache_selection(view: &CollectionVersion, query: &Value) -> Result<String> {
    let inner = relation_selections(query)?;
    let fields: Vec<String> = view
        .fields
        .iter()
        .filter(|field| !field.name.starts_with('_'))
        .filter_map(|field| {
            if field.kind.is_relation() {
                inner
                    .get(field.name.as_str())
                    .map(|inner| format!("{} {{ {inner} }}", field.name))
            } else {
                Some(field.name.clone())
            }
        })
        .collect();
    Ok(fields.join(" "))
}

/// Each relation field of the source query, by the name it is returned
/// under, with its inner field list.
fn relation_selections(query: &Value) -> Result<RapidHashMap<&str, String>> {
    let source_fields = query
        .get("Fields")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Other("view QuerySource.Query missing 'Fields' array".to_string()))?;
    let mut selections = RapidHashMap::new();
    for field in source_fields {
        let Some(inner) = field.get("Fields").and_then(Value::as_array) else {
            continue;
        };
        let name = field
            .get("Name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let returned_as = field.get("Alias").and_then(Value::as_str).unwrap_or(name);
        let inner: Vec<String> = inner
            .iter()
            .map(|inner| {
                let name = inner
                    .get("Name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                match inner.get("Alias").and_then(Value::as_str) {
                    Some(alias) => format!("{alias}: {name}"),
                    None => name.to_string(),
                }
            })
            .collect();
        selections.insert(returned_as, inner.join(" "));
    }
    Ok(selections)
}

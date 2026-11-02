use serde_json::Value;

use super::number::{canonical_text, json_eq};
use super::pointer::pointer_mut;
use super::ScenarioError;

fn sort_by_text(items: &mut [Value]) {
    items.sort_by_cached_key(canonical_text);
}

/// A copy with the arrays at `paths` sorted by canonical JSON.
pub fn normalized_result(value: &Value, paths: &[String]) -> Result<Value, ScenarioError> {
    let mut copy = value.clone();
    for path in paths {
        let selected = pointer_mut(&mut copy, path)
            .map_err(|e| ScenarioError::Path(format!("unordered path {path:?}: {e}")))?;
        let array = selected.as_array_mut().ok_or_else(|| {
            ScenarioError::Path(format!("unordered path {path:?} must identify an array"))
        })?;
        sort_by_text(array);
    }
    Ok(copy)
}

fn walk(value: &mut Value) {
    match value {
        Value::Object(map) => map.values_mut().for_each(walk),
        Value::Array(items) => {
            items.iter_mut().for_each(walk);
            if items.iter().all(Value::is_object) {
                sort_by_text(items);
            }
        }
        _ => {}
    }
}

/// A copy with every array of objects under each root sorted.
pub fn normalized_documents(value: &Value, paths: &[String]) -> Result<Value, ScenarioError> {
    let mut copy = value.clone();
    for path in paths {
        let selected = pointer_mut(&mut copy, path)?;
        let documents = selected
            .as_array_mut()
            .filter(|d| d.iter().all(Value::is_object));
        match documents {
            Some(documents) => documents.iter_mut().for_each(walk),
            None => {
                return Err(ScenarioError::Path(format!(
                    "unorderedDocuments path {path:?} must identify a document array"
                )))
            }
        }
        // The root array itself is a document array: order it too.
        if let Some(root) = pointer_mut(&mut copy, path)?.as_array_mut() {
            sort_by_text(root);
        }
    }
    Ok(copy)
}

pub fn equal_step_results(
    a: &Value,
    b: &Value,
    unordered: &[String],
    unordered_documents: &[String],
) -> Result<bool, ScenarioError> {
    let a = normalized_result(&normalized_documents(a, unordered_documents)?, unordered)?;
    let b = normalized_result(&normalized_documents(b, unordered_documents)?, unordered)?;
    Ok(json_eq(&a, &b))
}

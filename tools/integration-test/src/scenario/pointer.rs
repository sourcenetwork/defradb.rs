use rapidhash::RapidHashMap;
use serde_json::Value;

use super::ScenarioError;

fn tokens(path: &str) -> Result<Vec<String>, ScenarioError> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    let rest = path
        .strip_prefix('/')
        .ok_or_else(|| ScenarioError::Path(format!("invalid JSON pointer: {path}")))?;
    Ok(rest
        .split('/')
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
        .collect())
}

fn index(token: &str, len: usize, path: &str) -> Result<usize, ScenarioError> {
    token
        .parse::<usize>()
        .ok()
        .filter(|i| *i < len)
        .ok_or_else(|| ScenarioError::Path(format!("invalid array path {path}")))
}

pub fn pointer<'a>(value: &'a Value, path: &str) -> Result<&'a Value, ScenarioError> {
    let mut current = value;
    for token in tokens(path)? {
        current = match current {
            Value::Object(map) => map
                .get(&token)
                .ok_or_else(|| ScenarioError::Path(format!("missing path {path}")))?,
            Value::Array(items) => &items[index(&token, items.len(), path)?],
            _ => return Err(ScenarioError::Path(format!("cannot traverse {path}"))),
        };
    }
    Ok(current)
}

pub fn pointer_mut<'a>(value: &'a mut Value, path: &str) -> Result<&'a mut Value, ScenarioError> {
    let mut current = value;
    for token in tokens(path)? {
        current = match current {
            Value::Object(map) => map
                .get_mut(&token)
                .ok_or_else(|| ScenarioError::Path(format!("missing path {path}")))?,
            Value::Array(items) => {
                let i = index(&token, items.len(), path)?;
                &mut items[i]
            }
            _ => return Err(ScenarioError::Path(format!("cannot traverse {path}"))),
        };
    }
    Ok(current)
}

/// Replaces `{"$ref": name}` and `{"$concat": [..]}` objects with the value
/// they name, anywhere in `value`.
pub fn resolve(
    value: &Value,
    captures: &RapidHashMap<String, Value>,
) -> Result<Value, ScenarioError> {
    match value {
        Value::Object(map) => {
            if let (1, Some(parts)) = (map.len(), map.get("$concat")) {
                let items = parts
                    .as_array()
                    .filter(|items| !items.is_empty())
                    .ok_or_else(|| {
                        ScenarioError::Resolve(
                            "$concat requires a nonempty array of strings or string references"
                                .to_owned(),
                        )
                    })?;
                let mut result = String::new();
                for item in items {
                    match resolve(item, captures)? {
                        Value::String(text) => result.push_str(&text),
                        _ => {
                            return Err(ScenarioError::Resolve(
                                "$concat parts must resolve to strings".to_owned(),
                            ))
                        }
                    }
                }
                return Ok(Value::String(result));
            }
            if let (1, Some(reference)) = (map.len(), map.get("$ref")) {
                let name = reference
                    .as_str()
                    .ok_or_else(|| ScenarioError::Resolve("$ref must be a string".to_owned()))?;
                return captures
                    .get(name)
                    .cloned()
                    .ok_or_else(|| ScenarioError::Resolve(format!("unknown capture: {name}")));
            }
            let mut result = serde_json::Map::new();
            for (key, child) in map {
                result.insert(key.clone(), resolve(child, captures)?);
            }
            Ok(Value::Object(result))
        }
        Value::Array(items) => items
            .iter()
            .map(|item| resolve(item, captures))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        other => Ok(other.clone()),
    }
}

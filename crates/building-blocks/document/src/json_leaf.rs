//! JSON leaf value types for indexing
//!
//! Represents scalar values extracted from JSON with their paths,
//! ready for encoding as index keys.

use crate::json_path::JsonPath;
use serde::{Deserialize, Serialize};

/// Scalar value extracted from JSON for indexing.
///
/// JSON numbers are always stored as f64 to match Go behavior where
/// JSON numbers are unmarshaled as float64.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum JsonScalarValue {
    /// JSON null
    Null,
    /// JSON boolean
    Bool(bool),
    /// JSON number (stored as f64)
    Number(f64),
    /// JSON string
    String(String),
    /// Sentinel value for path lower bound (comes before all values for a path)
    /// Used for constraining range scans to a specific JSON path.
    PathMin,
    /// Sentinel value for path upper bound (comes after all values for a path)
    /// Used for constraining range scans to a specific JSON path.
    PathMax,
    /// Index-only marker under an empty array: its path is the array's path
    /// plus an index, the path the array's elements would have. It is never a
    /// JSON value, and only an `_all` scan looks it up, since `_all` over an
    /// empty array is true.
    EmptyArray,
}

impl JsonScalarValue {
    /// Create from a serde_json::Value, returning None for non-scalar values.
    pub fn from_json_value(value: &serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::Null => Some(JsonScalarValue::Null),
            serde_json::Value::Bool(b) => Some(JsonScalarValue::Bool(*b)),
            serde_json::Value::Number(n) => n.as_f64().map(JsonScalarValue::Number),
            serde_json::Value::String(s) => Some(JsonScalarValue::String(s.clone())),
            // Objects and arrays are not scalar values
            serde_json::Value::Object(_) | serde_json::Value::Array(_) => None,
        }
    }
}

/// A JSON leaf value with its path, ready for indexing.
///
/// This is an intermediate type used during index key generation.
/// JSON values are traversed and each leaf scalar is paired with
/// its path through the JSON structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonLeafValue {
    /// Path from root to this value
    pub path: JsonPath,
    /// The scalar value at this path
    pub value: JsonScalarValue,
}

impl JsonLeafValue {
    /// Create a new JSON leaf value.
    pub fn new(path: JsonPath, value: JsonScalarValue) -> Self {
        Self { path, value }
    }

    /// Create from a path and serde_json::Value, returning None for non-scalars.
    pub fn from_json(path: JsonPath, value: &serde_json::Value) -> Option<Self> {
        JsonScalarValue::from_json_value(value).map(|scalar| Self::new(path, scalar))
    }

    /// Whether this is the empty-array marker, which no filter but `_all`
    /// matches and no unique check counts.
    pub fn is_empty_array_marker(&self) -> bool {
        self.value == JsonScalarValue::EmptyArray
    }
}

/// The empty-array marker of every empty array in `json`, at the path its
/// elements would have.
pub fn json_empty_array_markers(json: &serde_json::Value) -> Vec<JsonLeafValue> {
    let mut markers = Vec::new();
    collect_empty_arrays(json, &JsonPath::new(), &mut markers);
    markers
}

fn collect_empty_arrays(
    value: &serde_json::Value,
    path: &JsonPath,
    markers: &mut Vec<JsonLeafValue>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                collect_empty_arrays(child, &path.append_property(key), markers);
            }
        }
        serde_json::Value::Array(elements) if elements.is_empty() => {
            markers.push(JsonLeafValue::new(
                path.append_index(),
                JsonScalarValue::EmptyArray,
            ));
        }
        serde_json::Value::Array(elements) => {
            let element_path = path.append_index();
            for element in elements {
                collect_empty_arrays(element, &element_path, markers);
            }
        }
        _ => {}
    }
}

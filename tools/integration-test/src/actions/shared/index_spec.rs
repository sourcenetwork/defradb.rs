use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One indexed field.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexField {
    pub name: String,
    pub descending: bool,
}

/// An index to create. `ordered` and `vector` carry the index's own option
/// objects.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexSpec {
    pub name: String,
    pub unique: bool,
    pub fields: Vec<IndexField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordered: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector: Option<Value>,
}

//! What document writes take and answer.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How documents are added: which of them is encrypted at rest.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddOptions {
    /// Encrypt every field of each document.
    pub encrypt_doc: bool,
    /// Encrypt only these fields.
    pub encrypted_fields: Vec<String>,
}

/// Which documents a filtered write acts on: a filter object, or its GraphQL
/// source text. Anything else is Go's `unsupported filter type`.
#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    Conditions(serde_json::Map<String, Value>),
    Source(String),
    Unsupported(Value),
}

impl From<Value> for Filter {
    fn from(value: Value) -> Self {
        match value {
            Value::Object(conditions) => Self::Conditions(conditions),
            Value::String(source) => Self::Source(source),
            other => Self::Unsupported(other),
        }
    }
}

/// The documents a filtered write changed: Go's `client.UpdateResult` and
/// `client.DeleteResult`, whose fields marshal capitalised.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentsResult {
    #[serde(rename = "Count")]
    pub count: usize,
    #[serde(rename = "DocIDs")]
    pub doc_ids: Vec<String>,
}

impl From<Vec<String>> for DocumentsResult {
    fn from(doc_ids: Vec<String>) -> Self {
        Self {
            count: doc_ids.len(),
            doc_ids,
        }
    }
}

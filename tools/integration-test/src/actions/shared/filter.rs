use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A document filter: its kind, and the filter body for the kinds that have one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

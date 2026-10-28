use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One WASM transform and its arguments.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LensModule {
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "Arguments")]
    pub arguments: Value,
}

/// A lens: the transforms it applies in order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lens {
    #[serde(rename = "Lenses", default, skip_serializing_if = "Option::is_none")]
    pub lenses: Option<Vec<LensModule>>,
}

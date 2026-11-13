use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A queued collection action to wait for.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExpectedAction {
    #[serde(rename = "Action")]
    pub action: u64,
    #[serde(rename = "CollectionID")]
    pub collection_id: String,
    #[serde(rename = "Status")]
    pub status: u64,
}

impl ExpectedAction {
    /// Whether the action list `actions` holds this action.
    pub fn listed_in(&self, actions: &Value) -> bool {
        actions.as_array().into_iter().flatten().any(|action| {
            action["Action"] == json!(self.action)
                && action["CollectionID"] == json!(self.collection_id)
                && action["Status"] == json!(self.status)
        })
    }
}

use serde::{Deserialize, Serialize};

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

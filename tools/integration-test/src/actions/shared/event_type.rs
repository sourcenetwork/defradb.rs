use serde::{Deserialize, Serialize};

/// Whether a peer joined or left a topic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    #[default]
    #[serde(rename = "JOINED")]
    Joined,
    #[serde(rename = "LEFT")]
    Left,
}

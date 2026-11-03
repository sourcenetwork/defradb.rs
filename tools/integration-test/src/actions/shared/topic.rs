use crate::scenario::Text;
use serde::{Deserialize, Serialize};

/// A pubsub topic and the peers expected on it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Topic {
    pub topic: Text,
    pub peers: Vec<String>,
}

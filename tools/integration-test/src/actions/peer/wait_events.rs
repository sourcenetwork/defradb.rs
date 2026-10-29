use crate::actions::shared::{EventType, Topic};
use crate::scenario::{Common, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use testo::prelude::*;

/// Waits for peers to join or leave topics, or starts listening for the peer events.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForPeersEvents {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_type: Option<EventType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<Topic>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub start: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for WaitForPeersEvents {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }
}

impl Action for WaitForPeersEvents {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

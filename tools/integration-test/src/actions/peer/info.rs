use crate::scenario::{Common, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use testo::prelude::*;

/// Reads a node's peer information: its address, or the validated information, or only its peer ID.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub validated: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub id_only: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for PeerInfo {
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

impl Action for PeerInfo {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

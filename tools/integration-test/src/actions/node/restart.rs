use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Restarts a node; with `stop` only the stopping half, the start comes as a later `Restart`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Restart {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stop: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for Restart {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run_mut(&self, state: &mut State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        match self.stop {
            true => {
                state.node(self.common.node())?;
                state.stop_node(self.common.node(), true);
            }
            false => state.restart_node(self.common.node())?,
        }
        Ok(json!({}))
    }
}

impl Action for Restart {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

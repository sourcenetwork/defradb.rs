use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

use super::StreamNodeSpec;
use crate::client_type::ClientType;
use crate::nodes::NodeSpec;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// Starts an in-process pinc node serving the given stream protocols.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewStreamNode {
    #[serde(flatten)]
    pub stream: StreamNodeSpec,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for NewStreamNode {
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
        if state.client_type != Some(ClientType::InProcess) {
            return Err(Failure::unsupported(format!(
                "{}: stream protocols are an embedder API with no HTTP, CLI or Go surface",
                Self::name()
            )));
        }
        let spec = NodeSpec {
            stream: Some(self.stream.clone()),
            ..NodeSpec::default()
        };
        state
            .start_stream_node(self.common.node(), spec)
            .map_err(Failure::operation)?;
        Ok(json!({}))
    }
}

impl Action for NewStreamNode {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

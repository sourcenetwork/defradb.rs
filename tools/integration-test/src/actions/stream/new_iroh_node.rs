use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

use super::IrohSpec;
use crate::client_type::ClientType;
use crate::nodes::NodeSpec;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// Starts an in-process iroh node serving the given stream protocols.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewIrohNode {
    #[serde(flatten)]
    pub iroh: IrohSpec,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for NewIrohNode {
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
            iroh: Some(self.iroh.clone()),
            ..NodeSpec::default()
        };
        state
            .start_iroh_node(self.common.node(), spec)
            .map_err(Failure::operation)?;
        Ok(json!({}))
    }
}

impl Action for NewIrohNode {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

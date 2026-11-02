use crate::actions::peer::support::{addresses_of, ambient, failed};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `peer.disconnect` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectPeers {
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for DisconnectPeers {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let targets: Vec<&str> = self.targets.iter().map(String::as_str).collect();
        let addresses = addresses_of(state, &targets)?;
        let node = state.node_on(self.common.node())?;
        state
            .block_on(node.disconnect_as(&ambient(), &addresses))
            .map_err(failed)?;
        Ok(json!({}))
    }
}

impl Action for DisconnectPeers {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for DisconnectPeers {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let targets: Vec<&str> = action.targets.iter().map(String::as_str).collect();
            let addresses = addresses_of(state, &targets)?;
            state
                .http_on(action.common.node(), |client| async move {
                    client.disconnect(&addresses).await
                })?
                .map_err(failed)?;
            Ok(json!({}))
        });
    }
}

impl CliRun for DisconnectPeers {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |_, _| {
            Err(Failure::unsupported(format!(
                "{}: the CLI has no disconnect command",
                Self::name()
            )))
        });
    }
}

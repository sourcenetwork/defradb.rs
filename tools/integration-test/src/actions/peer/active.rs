use crate::actions::peer::support::failed;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `peer.active` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivePeers {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ActivePeers {
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
        let identity = self.common.client_identity(Self::name())?;

        let node = state.node_on(self.common.node())?;
        let listed = state
            .block_on(node.active_peers_as(&state.context_as(None, identity)))
            .map_err(failed)?;
        Ok(json!({ "addresses": listed }))
    }
}

impl Action for ActivePeers {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for ActivePeers {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;

            let listed = state
                .http_as(action.common.node(), identity, |client| async move {
                    client.active_peers().await
                })?
                .map_err(failed)?;
            Ok(json!({ "addresses": listed }))
        });
    }
}

impl CliRun for ActivePeers {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;

            let printed = state
                .defra_client_as(
                    action.common.node(),
                    None,
                    identity,
                    &["p2p", "active-peers"],
                )
                .map_err(Failure::operation)?;
            Ok(json!({ "addresses": printed }))
        });
    }
}

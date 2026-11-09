use crate::actions::peer::support::failed;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `replicator.list` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListReplicators {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ListReplicators {
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
            .block_on(node.list_replicators_as(&state.context_as(None, identity)))
            .map_err(failed)?;
        Ok(json!({ "peerIDs": listed.iter().map(|r| r.id.as_str()).collect::<Vec<_>>() }))
    }
}

impl Action for ListReplicators {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for ListReplicators {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;

            let listed = state
                .http_as(action.common.node(), identity, |client| async move {
                    client.list_replicators().await
                })?
                .map_err(failed)?;
            Ok(json!({ "peerIDs": listed.iter().map(|r| r.id.as_str()).collect::<Vec<_>>() }))
        });
    }
}

impl CliRun for ListReplicators {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;

            let printed = state
                .defra_client_as(
                    action.common.node(),
                    None,
                    identity,
                    &["p2p", "replicator", "list"],
                )
                .map_err(Failure::operation)?;
            Ok(
                json!({ "peerIDs": printed.as_array().into_iter().flatten().map(|r| r["ID"].clone()).collect::<Vec<_>>() }),
            )
        });
    }
}

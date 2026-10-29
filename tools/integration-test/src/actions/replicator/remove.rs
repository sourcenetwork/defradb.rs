use crate::actions::peer::support::{addresses_of, ambient, failed};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `replicator.remove` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteReplicator {
    pub target: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for DeleteReplicator {
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
        let id = self.peer_id(state)?;
        let node = state.node_on(self.common.node())?;
        state
            .block_on(node.delete_replicator_as(&ambient(), &id, &[]))
            .map_err(failed)?;
        Ok(json!({}))
    }
}

impl DeleteReplicator {
    /// The peer ID of the node the action names.
    fn peer_id(&self, state: &State) -> Result<String, Failure> {
        let addresses = addresses_of(state, &[self.target.as_str()])?;
        Ok(addresses
            .first()
            .map(|a| crate::nodes::peer_id(a))
            .unwrap_or_default()
            .to_owned())
    }
}

impl Action for DeleteReplicator {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for DeleteReplicator {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let id = action.peer_id(state)?;
            state
                .http_on(action.common.node(), |client| async move {
                    client.delete_replicator(&id, &[]).await
                })?
                .map_err(failed)?;
            Ok(json!({}))
        });
    }
}

impl CliRun for DeleteReplicator {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |_, _| {
            Err(Failure::unsupported(format!(
                "{}: the CLI's replicator delete needs a collection",
                Self::name()
            )))
        });
    }
}

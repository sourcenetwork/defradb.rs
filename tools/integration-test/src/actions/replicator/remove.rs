use crate::actions::peer::support::{addresses_of, failed};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `replicator.remove` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteReplicator {
    pub target: String,
    /// The collections to stop pushing, by name or ID; none removes the replicator.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<Text>,
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
        let identity = self.common.client_identity(Self::name())?;
        let id = self.peer_id(state)?;
        let node = state.node_on(self.common.node())?;
        state
            .block_on(
                state
                    .scope_as(None, identity)
                    .scoped(&node)
                    .delete_replicator(&id, &self.collections()),
            )
            .map_err(failed)?;
        Ok(json!({}))
    }
}

impl DeleteReplicator {
    fn collections(&self) -> Vec<String> {
        self.collections
            .iter()
            .map(|collection| collection.as_str().to_owned())
            .collect()
    }

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
            let identity = action.common.client_identity(Self::name())?;
            let (id, collections) = (action.peer_id(state)?, action.collections());
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.delete_replicator(&id, &collections).await
                })?
                .map_err(failed)?;
            Ok(json!({}))
        });
    }
}

impl CliRun for DeleteReplicator {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let (id, collections) = (action.peer_id(state)?, action.collections().join(","));
            let mut args = vec!["p2p", "replicator", "delete"];
            if !collections.is_empty() {
                args.extend(["-c", collections.as_str()]);
            }
            args.push(id.as_str());
            state
                .defra_client_as(action.common.node(), None, identity, &args)
                .map_err(Failure::operation)?;
            Ok(json!({}))
        });
    }
}

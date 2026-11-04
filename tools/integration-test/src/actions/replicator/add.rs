use crate::actions::peer::support::{addresses_of, ambient, failed, REPLICATOR_COMPLETED};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `replicator.add` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddReplicator {
    pub target: String,
    /// The collections to push; every collection when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddReplicator {
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
        let addresses = addresses_of(state, &[self.target.as_str()])?;
        let (test_node, node) = (
            state.node(self.common.node())?,
            state.node_on(self.common.node())?,
        );
        test_node
            .replicator_completed(
                || {
                    state.block_on(node.add_replicator_as(
                        &ambient(),
                        &addresses,
                        &self.collections,
                    ))
                },
                REPLICATOR_COMPLETED,
            )
            .map_err(failed)?;
        Ok(json!({}))
    }
}

impl Action for AddReplicator {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for AddReplicator {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let addresses = addresses_of(state, &[action.target.as_str()])?;
            let test_node = state.node(action.common.node())?;
            test_node
                .replicator_completed(
                    || {
                        let collections = action.collections.clone();
                        state
                            .http_on(action.common.node(), |client| async move {
                                client.add_replicator(&addresses, &collections).await
                            })?
                            .map_err(failed)
                    },
                    REPLICATOR_COMPLETED,
                )
                .map(|()| json!({}))
        });
    }
}

impl CliRun for AddReplicator {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            if action.collections.is_empty() {
                return Err(Failure::unsupported(format!(
                    "{}: the CLI's replicator add needs a collection",
                    Self::name()
                )));
            }
            let addresses = addresses_of(state, &[action.target.as_str()])?;
            let collections = action.collections.join(",");
            let test_node = state.node(action.common.node())?;
            test_node
                .replicator_completed(
                    || {
                        let mut args =
                            vec!["p2p", "replicator", "add", "--collection", &collections];
                        args.extend(addresses.iter().map(String::as_str));
                        state
                            .defra_client_on(action.common.node(), None, &args)
                            .map(drop)
                            .map_err(Failure::operation)
                    },
                    REPLICATOR_COMPLETED,
                )
                .map(|()| json!({}))
        });
    }
}

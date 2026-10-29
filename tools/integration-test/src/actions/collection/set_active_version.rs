use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `collection.setActiveVersion` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetActiveCollectionVersion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(rename = "versionID")]
    pub version_id: Text,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for SetActiveCollectionVersion {
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
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .set_active_collection_version_as(
                        &state.context(self.txn),
                        self.version_id.as_str(),
                    ),
            )
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for SetActiveCollectionVersion {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for SetActiveCollectionVersion {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let (txn, version_id) = (
                state.txn_id(action.txn),
                action.version_id.as_str().to_owned(),
            );
            state
                .http_on(action.common.node(), |client| async move {
                    client
                        .set_active_collection_version(txn.as_deref(), &version_id)
                        .await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for SetActiveCollectionVersion {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let args = ["collection", "set-active", action.version_id.as_str()];
            state
                .defra_client_on(action.common.node(), action.txn, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}

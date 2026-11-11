use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `index.list` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListIndexes {
    pub collection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ListIndexes {
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
        if self.txn.is_some() {
            // The core reads and drops indexes outside any transaction yet.
            return Err(Failure::unsupported(format!(
                "{} argument txn",
                Self::name()
            )));
        }
        let indexes = state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .list_indexes_as(&state.context_as(self.txn, identity), &self.collection),
            )
            .map_err(|e| Failure::operation(e.to_string()))?;
        Ok(json!({ "indexes": indexes }))
    }
}

impl Action for ListIndexes {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for ListIndexes {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            if action.txn.is_some() {
                return Err(Failure::unsupported(format!(
                    "{} argument txn",
                    Self::name()
                )));
            }
            let collection = action.collection.clone();
            let indexes = state
                .http_as(action.common.node(), identity, |client| async move {
                    client.index_listing(&collection).await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            Ok(json!({ "indexes": indexes }))
        });
    }
}

impl CliRun for ListIndexes {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            if action.txn.is_some() {
                return Err(Failure::unsupported(format!(
                    "{} argument txn",
                    Self::name()
                )));
            }
            let args = ["index", "list", "--collection", action.collection.as_str()];
            let indexes = state
                .defra_client_as(action.common.node(), action.txn, identity, &args)
                .map_err(Failure::operation)?;
            Ok(json!({ "indexes": indexes }))
        });
    }
}

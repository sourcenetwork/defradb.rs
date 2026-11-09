use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `index.delete` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteIndex {
    pub collection: String,
    pub index_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for DeleteIndex {
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
        state
            .block_on(state.node_on(self.common.node())?.delete_index_as(
                &state.context_as(self.txn, identity),
                &self.collection,
                &self.index_name,
            ))
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for DeleteIndex {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for DeleteIndex {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            if action.txn.is_some() {
                return Err(Failure::unsupported(format!(
                    "{} argument txn",
                    Self::name()
                )));
            }
            let (collection, name) = (action.collection.clone(), action.index_name.clone());
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.index_delete(&collection, &name).await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for DeleteIndex {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            if action.txn.is_some() {
                return Err(Failure::unsupported(format!(
                    "{} argument txn",
                    Self::name()
                )));
            }
            let args = [
                "index",
                "delete",
                "--collection",
                action.collection.as_str(),
                "--name",
                action.index_name.as_str(),
            ];
            state
                .defra_client_as(action.common.node(), action.txn, identity, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}

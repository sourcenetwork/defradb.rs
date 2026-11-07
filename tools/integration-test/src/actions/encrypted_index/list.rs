use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `encryptedIndex.list` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEncryptedIndexes {
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

impl Op for ListEncryptedIndexes {
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
            return Err(Failure::unsupported(format!(
                "{} argument txn",
                Self::name()
            )));
        }
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .list_encrypted_indexes_as(&state.context_as(None, identity), &self.collection),
            )
            .map(|indexes| json!({ "indexes": indexes }))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for ListEncryptedIndexes {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for ListEncryptedIndexes {
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
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.encrypted_index_list(&collection).await
                })?
                .map(|indexes| json!({ "indexes": indexes }))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for ListEncryptedIndexes {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let args = [
                "encrypted-index",
                "list",
                "--collection",
                action.collection.as_str(),
            ];
            state
                .defra_client_as(action.common.node(), action.txn, identity, &args)
                .map(|indexes| json!({ "indexes": indexes }))
                .map_err(Failure::operation)
        });
    }
}

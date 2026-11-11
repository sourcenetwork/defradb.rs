use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `encryptedIndex.delete` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteEncryptedIndex {
    pub collection: String,
    pub field_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for DeleteEncryptedIndex {
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
                    .delete_encrypted_index_as(
                        &state.context_as(None, identity),
                        &self.collection,
                        &self.field_name,
                    ),
            )
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for DeleteEncryptedIndex {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for DeleteEncryptedIndex {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            if action.txn.is_some() {
                return Err(Failure::unsupported(format!(
                    "{} argument txn",
                    Self::name()
                )));
            }
            let (collection, field) = (action.collection.clone(), action.field_name.clone());
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.encrypted_index_delete(&collection, &field).await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for DeleteEncryptedIndex {
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
                "encrypted-index",
                "delete",
                "--collection",
                action.collection.as_str(),
                "--field",
                action.field_name.as_str(),
            ];
            state
                .defra_client_as(action.common.node(), action.txn, identity, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}

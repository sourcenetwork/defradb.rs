use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `encryptedIndex.create` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEncryptedIndex {
    pub collection: String,
    pub field_name: String,
    pub index_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for NewEncryptedIndex {
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
        state
            .block_on(state.node_on(self.common.node())?.add_encrypted_index_as(
                &state.context_as(self.txn, identity),
                &self.collection,
                &self.field_name,
            ))
            .map(|index| json!(index))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for NewEncryptedIndex {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for NewEncryptedIndex {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let (collection, field) = (action.collection.clone(), action.field_name.clone());
            let txn = state.txn_id(action.txn);
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client
                        .encrypted_index_add(txn.as_deref(), &collection, &field)
                        .await
                })?
                .map(|index| json!(index))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for NewEncryptedIndex {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let args = [
                "encrypted-index",
                "new",
                "--collection",
                action.collection.as_str(),
                "--field",
                action.field_name.as_str(),
            ];
            state
                .defra_client_as(action.common.node(), action.txn, identity, &args)
                .map_err(Failure::operation)
        });
    }
}

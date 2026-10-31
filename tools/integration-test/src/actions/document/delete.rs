use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `document.delete` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDoc {
    pub collection: String,
    #[serde(rename = "docID")]
    pub doc_id: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for DeleteDoc {
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
            .block_on(state.node_on(self.common.node())?.delete_doc(
                &state.context(self.txn),
                &self.collection,
                self.doc_id.as_str(),
            ))
            .map(|()| json!({ "deleted": true }))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for DeleteDoc {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for DeleteDoc {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let txn = state.txn_id(action.txn);
            let (collection, doc_id) =
                (action.collection.clone(), action.doc_id.as_str().to_owned());
            state
                .http_on(action.common.node(), |client| async move {
                    client
                        .delete_doc(txn.as_deref(), &collection, &doc_id)
                        .await
                })?
                .map(|()| json!({ "deleted": true }))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for DeleteDoc {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let args = [
                "document",
                "--collection-name",
                &action.collection,
                "delete",
                "--docID",
                action.doc_id.as_str(),
            ];
            state
                .defra_client_on(action.common.node(), action.txn, &args)
                .map(|_| json!({ "deleted": true }))
                .map_err(Failure::operation)
        });
    }
}

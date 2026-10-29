use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `document.update` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDoc {
    pub collection: String,
    #[serde(rename = "docID")]
    pub doc_id: Text,
    #[serde(rename = "document")]
    pub doc_map: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for UpdateDoc {
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
            .block_on(state.node_on(self.common.node())?.update_doc(
                &state.context(self.txn),
                &self.collection,
                self.doc_id.as_str(),
                &self.doc_map,
            ))
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for UpdateDoc {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for UpdateDoc {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let txn = state.txn_id(action.txn);
            let (collection, doc_id) =
                (action.collection.clone(), action.doc_id.as_str().to_owned());
            let patch = action.doc_map.clone();
            state
                .http_on(action.common.node(), |client| async move {
                    client
                        .update_doc(txn.as_deref(), &collection, &doc_id, &patch)
                        .await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for UpdateDoc {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let patch = action.doc_map.to_string();
            let args = [
                "document",
                "--collection-name",
                &action.collection,
                "update",
                "--docID",
                action.doc_id.as_str(),
                "--updater",
                &patch,
            ];
            state
                .defra_client_on(action.common.node(), action.txn, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}

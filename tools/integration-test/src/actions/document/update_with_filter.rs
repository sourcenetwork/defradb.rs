use crate::actions::shared::Filter;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `document.updateWithFilter` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWithFilter {
    pub collection: String,
    pub filter: Filter,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    pub updater: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for UpdateWithFilter {
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
            .block_on(state.node_on(self.common.node())?.update_with_filter(
                &state.context_as(self.txn, identity),
                &self.collection,
                &self.filter.to_core(),
                &self.updater,
            ))
            .map(|result| json!({ "count": result.count, "docIDs": result.doc_ids }))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for UpdateWithFilter {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for UpdateWithFilter {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let txn = state.txn_id(action.txn);
            let (collection, filter) = (action.collection.clone(), action.filter.to_core());
            let updater = action.updater.clone();
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client
                        .update_with_filter(txn.as_deref(), &collection, &filter, &updater)
                        .await
                })?
                .map(|result| json!({ "count": result.count, "docIDs": result.doc_ids }))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for UpdateWithFilter {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let filter = action
                .filter
                .to_cli()
                .map_err(|e| Failure::operation(e.to_string()))?;
            let args = [
                "document",
                "--collection-name",
                &action.collection,
                "update",
                "--filter",
                &filter,
                "--updater",
                &action.updater,
            ];
            let printed = state
                .defra_client_as(action.common.node(), action.txn, identity, &args)
                .map_err(Failure::operation)?;
            Ok(json!({ "count": printed["Count"], "docIDs": printed["DocIDs"] }))
        });
    }
}

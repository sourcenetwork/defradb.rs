use crate::actions::shared::Filter;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `document.deleteWithFilter` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteWithFilter {
    pub collection: String,
    pub filter: Filter,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for DeleteWithFilter {
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
            .block_on(state.node().delete_with_filter(
                &state.context(None),
                &self.collection,
                &self.filter.to_core(),
            ))
            .map(|result| json!({ "count": result.count, "docIDs": result.doc_ids }))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for DeleteWithFilter {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for DeleteWithFilter {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let (collection, filter) = (action.collection.clone(), action.filter.to_core());
            state
                .http(|client| async move {
                    client.delete_with_filter(None, &collection, &filter).await
                })
                .map(|result| json!({ "count": result.count, "docIDs": result.doc_ids }))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for DeleteWithFilter {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let filter = action
                .filter
                .to_cli()
                .map_err(|e| Failure::operation(e.to_string()))?;
            let args = [
                "document",
                "--collection-name",
                &action.collection,
                "delete",
                "--filter",
                &filter,
            ];
            let printed = state.defra_client(&args).map_err(Failure::operation)?;
            Ok(json!({ "count": printed["Count"], "docIDs": printed["DocIDs"] }))
        });
    }
}

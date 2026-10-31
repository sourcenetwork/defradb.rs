use crate::actions::shared::Filter;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use query::documents;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `collection.truncate` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Truncate {
    pub collection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Filter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for Truncate {
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
        let filter = self.filter.as_ref().map(Filter::to_core);
        state
            .block_on(state.node_on(self.common.node())?.truncate_collection_as(
                &state.context(self.txn),
                &self.collection,
                filter.as_ref(),
            ))
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Truncate {
    /// The filter as Go's HTTP and CLI clients send it: its JSON value.
    fn wire_filter(&self) -> Option<Value> {
        self.filter.as_ref().map(|filter| match filter.to_core() {
            documents::Filter::Conditions(conditions) => Value::Object(conditions),
            documents::Filter::Source(source) => Value::String(source),
            documents::Filter::Unsupported(value) => value,
        })
    }
}

impl Action for Truncate {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for Truncate {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let txn = state.txn_id(action.txn);
            let (collection, filter) = (action.collection.clone(), action.wire_filter());
            state
                .http_on(action.common.node(), |client| async move {
                    client
                        .collection_truncate(txn.as_deref(), &collection, filter.as_ref())
                        .await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for Truncate {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let filter = action.wire_filter().map(|filter| filter.to_string());
            let mut args = vec![
                "collection",
                "--collection-name",
                &action.collection,
                "truncate",
            ];
            if let Some(filter) = &filter {
                args.extend(["--filter", filter]);
            }
            state
                .defra_client_on(action.common.node(), action.txn, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}

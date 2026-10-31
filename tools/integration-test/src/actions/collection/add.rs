use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use testo::prelude::*;

/// Adds the collections declared in `sdl`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCollection {
    pub sdl: Text,
    /// The transaction to add them in, by the order it began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    /// The collection versions the request must return.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_results: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddCollection {
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
        self.check_arguments()?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .add_collection_as(&state.context(self.txn), self.sdl.as_str()),
            )
            .map(|_| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl AddCollection {
    /// Fails on the arguments no client supports yet.
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.plain(Self::name())?;
        if !self.expected_results.is_null() {
            return Err(Failure::unsupported(format!(
                "{} argument expectedResults",
                Self::name()
            )));
        }
        Ok(())
    }
}

impl Action for AddCollection {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for AddCollection {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            let sdl = action.sdl.as_str().to_owned();
            let txn = state.txn_id(action.txn);
            state
                .http_on(action.common.node(), |client| async move {
                    client.schema_add(&sdl, txn.as_deref()).await
                })?
                .map(|_| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for AddCollection {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            state
                .defra_client_on(
                    action.common.node(),
                    action.txn,
                    &["collection", "add", action.sdl.as_str()],
                )
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}

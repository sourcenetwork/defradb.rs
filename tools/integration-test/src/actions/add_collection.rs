use defra_core::RequestContext;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
        self.common.plain(Self::name())?;
        if !self.expected_results.is_null() {
            return Err(Failure::unsupported(format!(
                "{} argument expectedResults",
                Self::name()
            )));
        }
        if self.txn.is_some() {
            return Err(Failure::unsupported(format!(
                "{} argument transaction",
                Self::name()
            )));
        }
        state
            .block_on(
                state
                    .client()
                    .add_collection(&RequestContext::ambient(), self.sdl.as_str()),
            )
            .map(|_| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for AddCollection {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

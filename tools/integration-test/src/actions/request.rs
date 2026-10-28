use defra_core::RequestContext;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use testo::prelude::*;

/// Runs a GraphQL request and compares its `data` with `expected_data`
/// (unchecked when it is `null`). Errors the node reports in the response
/// count as the request's error, as in Go's suite.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    #[serde(rename = "query")]
    pub request: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_finite_floats: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_types: Option<Value>,
    /// The transaction to run it in, by the order it began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_extensions: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for Request {
    const GRAPHQL: bool = true;

    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn expected_extensions(&self) -> &Value {
        &self.expected_extensions
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let unsupported = [
            ("transaction", self.txn.is_some()),
            ("variables", self.variables.is_some()),
            ("nonFiniteFloats", self.non_finite_floats.is_some()),
            ("resultTypes", self.result_types.is_some()),
            ("operationName", self.operation_name.is_some()),
        ];
        if let Some((name, _)) = unsupported.iter().find(|(_, set)| *set) {
            return Err(Failure::unsupported(format!(
                "{} argument {name}",
                Self::name()
            )));
        }
        state
            .block_on(
                state
                    .client()
                    .exec_request(&RequestContext::ambient(), self.request.as_str()),
            )
            .map(|result| {
                let mut envelope = json!({"data": result.data});
                if !result.errors.is_empty() {
                    let errors: Vec<Value> = result
                        .errors
                        .iter()
                        .map(|m| json!({"message": m}))
                        .collect();
                    envelope["errors"] = Value::Array(errors);
                }
                envelope
            })
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for Request {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

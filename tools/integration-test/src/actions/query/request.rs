use defra_node::QueryRequest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::clients::{CliRun, HttpRun};
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
    /// `expected_data` on a Go node, where Go's answer differs on purpose.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data_go: Value,
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

    fn expected_data_go(&self) -> &Value {
        &self.expected_data_go
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn expected_extensions(&self) -> &Value {
        &self.expected_extensions
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        self.check_arguments()?;
        state
            .block_on(state.node_on(self.common.node())?.execute_request_as(
                &state.context(self.txn),
                QueryRequest::new(self.request.as_str()),
            ))
            .map(|response| {
                envelope(
                    response.data,
                    response.errors.iter().map(|e| e.message.as_str()),
                )
            })
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Request {
    /// Fails on the arguments no client supports yet.
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.plain(Self::name())?;
        let unsupported = [
            ("variables", self.variables.is_some()),
            ("nonFiniteFloats", self.non_finite_floats.is_some()),
            ("resultTypes", self.result_types.is_some()),
            ("operationName", self.operation_name.is_some()),
        ];
        match unsupported.iter().find(|(_, set)| *set) {
            Some((name, _)) => Err(Failure::unsupported(format!(
                "{} argument {name}",
                Self::name()
            ))),
            None => Ok(()),
        }
    }
}

/// A GraphQL response as the scenario compares it: its data, and the message
/// of each error.
fn envelope<'a, I>(data: Option<Value>, errors: I) -> Value
where
    I: Iterator<Item = &'a str>,
{
    let mut envelope = json!({"data": data.unwrap_or_default()});
    let errors: Vec<Value> = errors.map(|m| json!({"message": m})).collect();
    if !errors.is_empty() {
        envelope["errors"] = Value::Array(errors);
    }
    envelope
}

impl Action for Request {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for Request {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            let request = action.request.as_str().to_owned();
            let txn = state.txn_id(action.txn);
            state
                .http_on(action.common.node(), |client| async move {
                    client.graphql(&request, None, txn).await
                })?
                .map(|response| {
                    envelope(
                        response.data,
                        response.errors.iter().map(|e| e.message.as_str()),
                    )
                })
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for Request {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            state
                .defra_client_on(
                    action.common.node(),
                    action.txn,
                    &["query", action.request.as_str()],
                )
                .map_err(Failure::operation)
        });
    }
}

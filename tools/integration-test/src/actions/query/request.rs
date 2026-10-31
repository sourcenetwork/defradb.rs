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
        self.check_arguments()?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .execute_request_as(&state.context(self.txn), self.query_request()),
            )
            .map(|response| {
                envelope(
                    response.data,
                    response.errors.iter().map(|e| e.message.as_str()),
                )
            })
            .map_err(|e| Failure::operation(e.to_string()))
            .and_then(|envelope| self.typed(envelope))
    }
}

impl Request {
    /// `envelope` when every `resultTypes` path holds a `float32Array`: an
    /// array of numbers that are each a 32-bit float, written as its exact
    /// value or as its shortest decimal (Go's and defradb.rs's Float32
    /// rendering). That is the most a JSON answer can show of Go's
    /// `[]float32` assignability check.
    fn typed(&self, envelope: Value) -> Result<Value, Failure> {
        let Some(types) = self.result_types.as_ref().and_then(Value::as_object) else {
            return Ok(envelope);
        };
        for (pointer, kind) in types {
            if kind != "float32Array" {
                return Err(Failure::unsupported(format!("resultTypes type {kind}")));
            }
            let floats = envelope["data"]
                .pointer(pointer)
                .and_then(Value::as_array)
                .is_some_and(|items| {
                    items
                        .iter()
                        .all(|item| item.as_f64().is_some_and(is_float32))
                });
            if !floats {
                return Err(Failure::operation(format!(
                    "resultTypes: {pointer} is not a float32Array"
                )));
            }
        }
        Ok(envelope)
    }

    fn query_request(&self) -> QueryRequest {
        let request = QueryRequest::new(self.request.as_str());
        match &self.variables {
            Some(variables) => request.with_variables(variables.clone()),
            None => request,
        }
    }

    /// Fails on the arguments no client supports yet.
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.plain(Self::name())?;
        let unsupported = [
            ("nonFiniteFloats", self.non_finite_floats.is_some()),
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

/// Whether `value` is a 32-bit float, exactly or as its shortest decimal.
fn is_float32(value: f64) -> bool {
    let single = value as f32;
    f64::from(single) == value || single.to_string().parse::<f64>() == Ok(value)
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
            let variables = action.variables.clone();
            let txn = state.txn_id(action.txn);
            state
                .http_on(action.common.node(), |client| async move {
                    client.graphql(&request, variables, txn).await
                })?
                .map(|response| {
                    envelope(
                        response.data,
                        response.errors.iter().map(|e| e.message.as_str()),
                    )
                })
                .map_err(|e| Failure::operation(e.to_string()))
                .and_then(|envelope| action.typed(envelope))
        });
    }
}

impl CliRun for Request {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            let variables = action.variables.as_ref().map(Value::to_string);
            let mut args = vec!["query", action.request.as_str()];
            if let Some(variables) = &variables {
                args.extend(["--variables", variables.as_str()]);
            }
            state
                .defra_client_on(action.common.node(), action.txn, &args)
                .map_err(Failure::operation)
                .and_then(|envelope| action.typed(envelope))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::is_float32;

    #[test]
    fn float32_exact_or_shortest() {
        assert!(is_float32(0.1));
        assert!(is_float32(f64::from(0.1_f32)));
        assert!(is_float32(-3.0));
        assert!(!is_float32(0.123_456_789_012_345));
        assert!(!is_float32(1e300));
    }
}

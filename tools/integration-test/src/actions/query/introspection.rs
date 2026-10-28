use defra_node::QueryRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use testo::prelude::*;

use crate::actions::expect::expect_error;
use crate::clients::{CliRun, HttpRun};
use crate::test_case::State;

/// Runs a GraphQL introspection query and compares its `data` in full, or, with
/// `contains_data`, checks that `expected_data` is contained in it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IntrospectionRequest {
    pub request: String,
    pub expected_data: Value,
    pub expected_error: Option<String>,
    #[serde(default)]
    pub contains_data: bool,
    /// The transaction to run it in, by the order it began.
    pub txn: Option<usize>,
}

impl Action for IntrospectionRequest {
    type TState = State;

    fn execute(&self, state: &mut State) {
        assert!(self.txn.is_none(), "transactions are not supported yet");
        let response = state.block_on(
            state
                .node()
                .execute_request(QueryRequest::new(self.request.as_str())),
        );
        self.check(graphql_outcome(
            response.data,
            response.errors.iter().map(|e| e.message.as_str()),
        ));
    }
}

impl HttpRun for IntrospectionRequest {
    fn execute_http(&self, state: &mut State) {
        assert!(self.txn.is_none(), "transactions are not supported yet");
        let request = self.request.clone();
        let response = state
            .http(|client| async move { client.graphql(&request, None, None).await })
            .expect("run request");
        self.check(graphql_outcome(
            response.data,
            response.errors.iter().map(|e| e.message.as_str()),
        ));
    }
}

impl CliRun for IntrospectionRequest {
    fn execute_cli(&self, state: &mut State) {
        assert!(self.txn.is_none(), "transactions are not supported yet");
        let result = state.defra_client(&["query", &self.request]);
        self.check(result.map(|printed| printed["data"].clone()));
    }
}

/// A GraphQL response's data, or its errors' messages when it has any.
fn graphql_outcome<'a, I>(data: Option<Value>, errors: I) -> Result<Value, String>
where
    I: Iterator<Item = &'a str>,
{
    let errors: Vec<&str> = errors.collect();
    match errors.is_empty() {
        true => Ok(data.unwrap_or_default()),
        false => Err(errors.join("\n")),
    }
}

impl IntrospectionRequest {
    /// Checks what a client got back against what the action expects.
    fn check(&self, result: Result<Value, String>) {
        if let Some(data) = expect_error(result, self.expected_error.as_deref()) {
            if self.contains_data {
                assert!(
                    contains(&data, &self.expected_data),
                    "expected data to contain {}, got {data}",
                    self.expected_data
                );
            } else {
                assert_eq!(data, self.expected_data);
            }
        }
    }
}

/// Whether `actual` holds everything in `expected`: every expected key is present
/// and matches, and every expected array element matches some actual element.
fn contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => expected
            .iter()
            .all(|(key, value)| actual.get(key).is_some_and(|found| contains(found, value))),
        (Value::Array(actual), Value::Array(expected)) => expected
            .iter()
            .all(|wanted| actual.iter().any(|found| contains(found, wanted))),
        _ => actual == expected,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::contains;

    #[test]
    fn contains_ignores_extra_keys_and_elements() {
        let actual = json!({ "a": [{ "x": 1, "y": 2 }, { "x": 3 }], "b": null });
        assert!(contains(&actual, &json!({ "a": [{ "x": 3 }, { "x": 1 }] })));
    }

    #[test]
    fn contains_rejects_missing_or_different_values() {
        let actual = json!({ "a": [{ "x": 1 }], "b": 2 });
        assert!(!contains(&actual, &json!({ "c": 1 })));
        assert!(!contains(&actual, &json!({ "b": 3 })));
        assert!(!contains(&actual, &json!({ "a": [{ "x": 2 }] })));
        assert!(!contains(
            &actual,
            &json!({ "a": [{ "x": 1 }, { "x": 1, "z": 0 }] })
        ));
    }
}

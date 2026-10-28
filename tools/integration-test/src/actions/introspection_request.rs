use serde_json::Value;
use testo::prelude::*;

use super::expect::expect_error;
use crate::test_case::State;

/// Runs a GraphQL introspection query and compares its `data` in full.
#[derive(Clone, Debug, Default)]
pub struct IntrospectionRequest {
    pub request: &'static str,
    pub expected_data: Value,
    pub expected_error: Option<&'static str>,
}

impl Action for IntrospectionRequest {
    type TState = State;

    fn execute(&self, state: &mut State) {
        let response = workers::block_on(state.node.execute(self.request)).expect("run query");
        let result = match response.errors.is_empty() {
            true => Ok(response.data.unwrap_or_default()),
            false => Err(response
                .errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("\n")),
        };
        if let Some(data) = expect_error(result, self.expected_error) {
            assert_eq!(data, self.expected_data);
        }
    }
}

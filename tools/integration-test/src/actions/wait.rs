use std::time::{Duration, Instant};

use crate::actions::shared::ExpectedAction;
use crate::client_type::ClientType;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// How long a wait for a collection action polls the action list.
const ACTION_TIMEOUT: Duration = Duration::from_secs(10);
const ACTION_POLL: Duration = Duration::from_millis(20);

/// Waits a number of milliseconds, or for a queued collection action.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wait {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_action: Option<ExpectedAction>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for Wait {
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
        if let Some(millis) = self.millis {
            std::thread::sleep(Duration::from_millis(millis));
        }
        if let Some(expected) = &self.expected_action {
            self.await_action(state, expected)?;
        }
        Ok(json!({}))
    }
}

impl Wait {
    /// Poll the node's action list, through the test's client, until it
    /// lists `expected`.
    fn await_action(&self, state: &State, expected: &ExpectedAction) -> Result<(), Failure> {
        let deadline = Instant::now() + ACTION_TIMEOUT;
        loop {
            let actions = self.actions(state)?;
            let found = actions.as_array().into_iter().flatten().any(|action| {
                action["Action"] == json!(expected.action)
                    && action["CollectionID"] == json!(expected.collection_id)
                    && action["Status"] == json!(expected.status)
            });
            if found {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Failure::operation(format!(
                    "no action {expected:?} after {ACTION_TIMEOUT:?}; listed {actions}"
                )));
            }
            std::thread::sleep(ACTION_POLL);
        }
    }

    /// The node's action list as JSON.
    fn actions(&self, state: &State) -> Result<Value, Failure> {
        let node = self.common.node();
        match state.client_type() {
            ClientType::InProcess => {
                let listed = state
                    .block_on(state.node_on(node)?.list_actions_as(&state.context(None)))
                    .map_err(|e| Failure::operation(e.to_string()))?;
                serde_json::to_value(listed).map_err(|e| Failure::operation(e.to_string()))
            }
            ClientType::Cli => state
                .defra_client_as(node, None, None, &["action", "list"])
                .map_err(Failure::operation),
            ClientType::RustHttp | ClientType::GoHttp => state
                .http_on(node, |client| async move { client.action_list().await })?
                .map(|listed| json!(listed))
                .map_err(|e| Failure::operation(e.to_string())),
        }
    }
}

impl Action for Wait {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

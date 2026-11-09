use crate::actions::shared::ExpectedAction;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Lists the queued collection actions, or starts listening for their events, or waits for the list to fill.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListActions {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub listen: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "expectedAction")]
    pub after_action: Option<ExpectedAction>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ListActions {
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
        let identity = self.check_arguments()?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .list_actions_as(&state.context_as(None, identity)),
            )
            .map(|actions| json!({ "actions": actions }))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl ListActions {
    /// Fails on listening, which needs the node's action events.
    fn check_arguments(&self) -> Result<Option<u64>, Failure> {
        let identity = self.common.client_identity(Self::name())?;
        match self.listen || self.after_action.is_some() {
            true => Err(Failure::unsupported(format!(
                "{} argument listen",
                Self::name()
            ))),
            false => Ok(identity),
        }
    }
}

impl HttpRun for ListActions {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.check_arguments()?;
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.action_list().await
                })?
                .map(|actions| json!({ "actions": actions }))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for ListActions {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.check_arguments()?;
            state
                .defra_client_as(action.common.node(), None, identity, &["action", "list"])
                .map(|actions| json!({ "actions": actions }))
                .map_err(Failure::operation)
        });
    }
}

impl Action for ListActions {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

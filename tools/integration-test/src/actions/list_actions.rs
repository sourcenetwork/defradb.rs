use crate::actions::shared::ExpectedAction;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Lists the queued collection actions, or starts listening for their events, or waits for the list to fill.
///
/// `listen` is Go's `action.events.start`: it records the node's action
/// events, which `Wait { expectedAction }` then reads. `expectedAction` is
/// Go's `action.listAfterEventAsync`, which lists while an action runs.
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
        let identity = self.common.client_identity(Self::name())?;
        if let Some(staged) = self.staged(state) {
            return staged;
        }
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
    /// The staged forms' results; `None` for a plain listing.
    ///
    /// A list taken while an action runs is refused: the node can finish a
    /// truncate or a view refresh before any list reads it, and nothing holds
    /// an action in progress, so the result would depend on timing.
    fn staged(&self, state: &State) -> Option<Result<Value, Failure>> {
        if self.listen {
            return Some(self.listen(state).map(|()| json!({})));
        }
        if self.after_action.is_some() {
            return Some(Err(Failure::unsupported(format!(
                "{} argument expectedAction: nothing holds an action in progress for a list to see",
                Self::name()
            ))));
        }
        None
    }

    /// Go's `action.events.start`: record the node's action events from now.
    fn listen(&self, state: &State) -> Result<(), Failure> {
        state
            .node(self.common.node())?
            .listen_actions()
            .map(drop)
            .map_err(Failure::unsupported)
    }
}

impl HttpRun for ListActions {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            if let Some(staged) = action.staged(state) {
                return staged;
            }
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
            let identity = action.common.client_identity(Self::name())?;
            if let Some(staged) = action.staged(state) {
                return staged;
            }
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

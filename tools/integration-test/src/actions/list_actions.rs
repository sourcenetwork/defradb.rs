use std::sync::Arc;
use std::time::Instant;

use crate::actions::shared::ExpectedAction;
use crate::actions::wait::{listed_actions, ACTION_POLL, ACTION_TIMEOUT};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use regolith::sync::Atom;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Lists the queued collection actions, or starts listening for their events, or waits for the list to fill.
///
/// The three staged forms are Go's: `listen` (`action.events.start`) opens
/// the listening, `expectedAction` (`action.listAfterEventAsync`) lists in the
/// background once the action shows, and the next plain `ListActions`
/// (`action.awaitList`) reads that list.
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

/// What a background `ListActions` has done: `None` when nothing started,
/// `Some(None)` while it runs, then the list it took.
type Progress = Option<Option<Result<Value, Failure>>>;

/// A background `ListActions`'s [`Progress`]. Shared by every clone of the
/// state.
#[derive(Clone, Default)]
pub struct PendingList(Arc<Atom<Progress>>);

impl PendingList {
    fn start(&self) {
        self.0.store(Some(None));
    }

    fn finish(&self, listed: Result<Value, Failure>) {
        self.0.store(Some(Some(listed)));
    }

    /// The background list, once it is taken; `None` when none was started.
    fn take(&self) -> Option<Result<Value, Failure>> {
        let deadline = Instant::now() + ACTION_TIMEOUT * 2;
        loop {
            match self.0.load_clone() {
                None => return None,
                Some(Some(listed)) => {
                    self.0.store(None);
                    return Some(listed);
                }
                Some(None) if Instant::now() >= deadline => {
                    return Some(Err(Failure::operation(
                        "the background ListActions did not finish".to_owned(),
                    )));
                }
                Some(None) => std::thread::sleep(ACTION_POLL),
            }
        }
    }
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
    fn staged(&self, state: &State) -> Option<Result<Value, Failure>> {
        if self.listen {
            return Some(self.listen(state).map(|()| json!({})));
        }
        if let Some(expected) = &self.after_action {
            return Some(self.list_after(state, expected.clone()).map(|()| json!({})));
        }
        state
            .pending_list
            .take()
            .map(|listed| listed.map(|actions| json!({ "actions": actions })))
    }

    /// List the node's actions in the background once its event for
    /// `expected` arrives, or once the wait for it times out, as Go's async
    /// `Wait { Action }` then `ListActions` do.
    fn list_after(&self, state: &State, expected: ExpectedAction) -> Result<(), Failure> {
        let node = self.common.node().to_owned();
        let events = state
            .node(&node)?
            .listen_actions()
            .map_err(Failure::unsupported)?;
        let state = state.clone();
        state.pending_list.start();
        std::thread::spawn(move || {
            // Timing out lists anyway, as Go's `Wait` ends after its duration.
            let _ = state.block_on(events.arrived(&expected, ACTION_TIMEOUT));
            state.pending_list.finish(listed_actions(&state, &node));
        });
        Ok(())
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

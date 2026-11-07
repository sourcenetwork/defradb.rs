use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `view.add` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddView {
    pub query: Text,
    pub sdl: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(rename = "transformCID")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform_cid: Option<Text>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddView {
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
        self.check_arguments()?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .add_view(self.query.as_str(), self.sdl.as_str()),
            )
            .map(|_| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for AddView {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl AddView {
    /// Fails on the arguments no client supports yet.
    fn check_arguments(&self) -> Result<(), Failure> {
        self.common.plain(Self::name())?;
        match self.txn.is_some() || self.transform_cid.is_some() {
            true => Err(Failure::unsupported(format!(
                "{} in a transaction or with a transform",
                Self::name()
            ))),
            false => Ok(()),
        }
    }
}

const VIEWS_DISABLED: &str = "View operations are not enabled.";

// A server without view routes cannot run the op, which is not a failure of it.
fn views_failure(message: String) -> Failure {
    match message.contains(VIEWS_DISABLED) {
        true => Failure::unsupported(format!("{}: {VIEWS_DISABLED}", AddView::name())),
        false => Failure::operation(message),
    }
}

impl HttpRun for AddView {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            let (query, sdl) = (
                action.query.as_str().to_owned(),
                action.sdl.as_str().to_owned(),
            );
            state
                .http_on(action.common.node(), |client| async move {
                    client.view_add(&query, &sdl, None).await
                })?
                .map(|_| json!({}))
                .map_err(|e| views_failure(e.to_string()))
        });
    }
}

impl CliRun for AddView {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.check_arguments()?;
            state
                .defra_client_on(
                    action.common.node(),
                    None,
                    &[
                        "view",
                        "add",
                        "--query",
                        action.query.as_str(),
                        "--sdl",
                        action.sdl.as_str(),
                    ],
                )
                .map(|_| json!({}))
                .map_err(views_failure)
        });
    }
}

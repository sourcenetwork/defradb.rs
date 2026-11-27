//! The `debug.dump` op: Go's `PrintDump`, every key of the node's store.

use crate::client_type::ClientType;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `debug.dump` op. Its result says whether the dump listed any key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintDump {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for PrintDump {
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
        let lines = state
            .block_on(state.node_on(self.common.node())?.print_dump())
            .map_err(|e| Failure::operation(e.to_string()))?;
        Ok(listed(!lines.is_empty()))
    }
}

fn listed(any: bool) -> Value {
    json!({ "listsKeys": any })
}

/// The lines a node's HTTP API returned.
fn printed(lines: Value) -> Result<Value, Failure> {
    match lines.as_array() {
        Some(lines) => Ok(listed(!lines.is_empty())),
        None => Err(Failure::operation(format!("no dump lines in {lines}"))),
    }
}

/// A Go node logs its dump where it runs and returns nothing.
fn refuse_go(state: &State) -> Result<(), Failure> {
    match state.client_type() {
        ClientType::GoHttp => Err(Failure::unsupported(
            "debug.dump on a Go node: it logs the store and returns no lines",
        )),
        _ => Ok(()),
    }
}

impl Action for PrintDump {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for PrintDump {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            refuse_go(state)?;
            let lines = state
                .http_as(action.common.node(), identity, |client| async move {
                    client.dump().await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            printed(lines)
        });
    }
}

impl CliRun for PrintDump {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let lines = state
                .defra_client_as(action.common.node(), None, identity, &["dump"])
                .map_err(Failure::operation)?;
            printed(lines)
        });
    }
}

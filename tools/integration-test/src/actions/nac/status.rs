use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `nac.status` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetNACStatus {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for GetNACStatus {
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
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .nac_status_as(&state.context_as(None, identity)),
            )
            .map(|status| json!({ "status": status.to_string() }))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

/// Go's `NACStatusResult` as the scenarios read it.
fn status(printed: Value) -> Result<Value, Failure> {
    match printed.get("Status").and_then(Value::as_str) {
        Some(status) => Ok(json!({ "status": status })),
        None => Err(Failure::operation(format!("no NAC status in {printed}"))),
    }
}

impl Action for GetNACStatus {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for GetNACStatus {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let printed = state
                .http_as(action.common.node(), identity, |client| async move {
                    client.nac_status().await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            status(printed)
        });
    }
}

impl CliRun for GetNACStatus {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let printed = state
                .defra_client_as(
                    action.common.node(),
                    None,
                    identity,
                    &["acp", "node", "status"],
                )
                .map_err(Failure::operation)?;
            status(printed)
        });
    }
}

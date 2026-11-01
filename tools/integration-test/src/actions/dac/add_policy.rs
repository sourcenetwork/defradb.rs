use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `dac.addPolicy` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDACPolicy {
    pub policy: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddDACPolicy {
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
                    .add_dac_policy_as(&state.context_as(None, identity), &self.policy),
            )
            .map(added)
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

/// Go's `AddPolicyResult` as the scenarios read it.
fn added(policy_id: String) -> Value {
    json!({ "policyID": policy_id })
}

impl Action for AddDACPolicy {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for AddDACPolicy {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let policy = action.policy.clone();
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.acp_add_policy(&policy).await
                })?
                .map(|response| added(response.policy_id))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for AddDACPolicy {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let args = ["acp", "document", "policy", "add", action.policy.as_str()];
            let printed = state
                .defra_client_as(action.common.node(), None, identity, &args)
                .map_err(Failure::operation)?;
            match printed.get("PolicyID").and_then(Value::as_str) {
                Some(policy_id) => Ok(added(policy_id.to_owned())),
                None => Err(Failure::operation(format!(
                    "the CLI printed no PolicyID: {printed}"
                ))),
            }
        });
    }
}

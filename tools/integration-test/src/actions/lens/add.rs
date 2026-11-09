use crate::actions::shared::Lens;
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `lens.add` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLens {
    pub lens: Lens,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddLens {
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
                    .add_lens_as(&state.context_as(self.txn, identity), &self.lens.to_wire()),
            )
            .map(|id| json!({ "lensID": id }))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for AddLens {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for AddLens {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let (txn, lens) = (state.txn_id(action.txn), action.lens.to_wire());
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.add_lens(txn.as_deref(), &lens).await
                })?
                .map(|id| json!({ "lensID": id }))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for AddLens {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let lens = action.lens.to_wire().to_string();
            let printed = state
                .defra_client_as(
                    action.common.node(),
                    action.txn,
                    identity,
                    &["lens", "add", &lens],
                )
                .map_err(Failure::operation)?;
            Ok(json!({ "lensID": printed["lensId"] }))
        });
    }
}

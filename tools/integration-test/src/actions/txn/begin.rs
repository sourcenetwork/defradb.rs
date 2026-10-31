use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use defra_core::TxnId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `transaction.begin` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnBegin {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for TxnBegin {
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
        self.common.plain(Self::name())?;
        let id = state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .txn_begin(&state.context(None), self.read_only),
            )
            .map_err(|e| Failure::operation(e.to_string()))?;
        state.txns.push(id);
        Ok(json!({}))
    }
}

impl Action for TxnBegin {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for TxnBegin {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let read_only = action.read_only;
            let begun = state
                .http_on(action.common.node(), |client| async move {
                    client.tx_begin(read_only).await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            state.txns.push(TxnId(begun.id));
            Ok(json!({}))
        });
    }
}

impl CliRun for TxnBegin {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let args: &[&str] = match action.read_only {
                true => &["tx", "new", "--read-only"],
                false => &["tx", "new"],
            };
            let printed = state
                .defra_client_on(action.common.node(), None, args)
                .map_err(Failure::operation)?;
            let id = printed["id"].as_u64().ok_or_else(|| {
                Failure::operation(format!("defra client tx new printed no ID: {printed}"))
            })?;
            state.txns.push(TxnId(id));
            Ok(json!({}))
        });
    }
}

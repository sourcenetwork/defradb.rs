use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use defra_core::TxnId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `transaction.discard` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnDiscard {
    pub txn: usize,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for TxnDiscard {
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
        let id = self.id(state)?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .txn_discard(&state.context_as(None, identity), id),
            )
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl TxnDiscard {
    /// The ID of the transaction the action names.
    fn id(&self, state: &State) -> Result<TxnId, Failure> {
        state
            .txns
            .get(self.txn)
            .ok_or_else(|| Failure::operation(format!("no transaction {}", self.txn)))
    }
}

impl Action for TxnDiscard {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for TxnDiscard {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let id = action.id(state)?.to_string();
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.tx_discard(&id).await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for TxnDiscard {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let id = action.id(state)?.to_string();
            state
                .defra_client_as(
                    action.common.node(),
                    None,
                    identity,
                    &["tx", "discard", &id],
                )
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}

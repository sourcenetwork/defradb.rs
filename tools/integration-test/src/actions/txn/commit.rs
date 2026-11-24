use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use defra_core::TxnId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `transaction.commit` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnCommit {
    pub txn: usize,
    /// Commit this text as the ID in place of transaction `txn`'s, to check
    /// how a malformed ID is answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_id: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    /// The commit outcome the failure carries: `aborted` or `indeterminate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_commit_outcome: Option<String>,
    /// The CLI's exit status for the failure; not checked under other clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_exit_code: Option<i32>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for TxnCommit {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn expected_commit_outcome(&self) -> Option<&str> {
        self.expected_commit_outcome.as_deref()
    }

    fn expected_exit_code(&self) -> Option<i32> {
        self.expected_exit_code
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        let identity = self.common.client_identity(Self::name())?;
        if self.raw_id.is_some() {
            return Err(Failure::unsupported(format!(
                "{} argument rawId in process: the node takes a parsed ID",
                Self::name()
            )));
        }
        let id = self.id(state)?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .txn_commit(&state.context_as(None, identity), id),
            )
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()).with_outcome(e.commit_outcome()))
    }
}

impl TxnCommit {
    /// The ID of the transaction the action names.
    fn id(&self, state: &State) -> Result<TxnId, Failure> {
        state
            .txns
            .get(self.txn)
            .ok_or_else(|| Failure::operation(format!("no transaction {}", self.txn)))
    }

    /// The ID the commit sends over the wire.
    fn wire_id(&self, state: &State) -> Result<String, Failure> {
        match &self.raw_id {
            Some(raw) => Ok(raw.clone()),
            None => self.id(state).map(|id| id.to_string()),
        }
    }
}

impl Action for TxnCommit {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for TxnCommit {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let id = action.wire_id(state)?;
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.tx_commit(&id).await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()).with_outcome(e.commit_outcome()))
        });
    }
}

impl CliRun for TxnCommit {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let id = action.wire_id(state)?;
            state
                .defra_client_status_as(
                    action.common.node(),
                    None,
                    identity,
                    &["tx", "commit", &id],
                )
                .map(|_| json!({}))
                .map_err(Failure::cli)
        });
    }
}

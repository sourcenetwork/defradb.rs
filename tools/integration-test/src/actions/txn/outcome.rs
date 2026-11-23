use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use defra_core::TxnId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use testo::prelude::*;

/// The HTTP transaction-outcome query.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxnOutcome {
    #[serde(default)]
    pub txn: usize,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub malformed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for TxnOutcome {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }
}

impl TxnOutcome {
    fn id(&self, state: &State) -> Result<TxnId, Failure> {
        state
            .txns
            .get(self.txn)
            .ok_or_else(|| Failure::operation(format!("no transaction {}", self.txn)))
    }
}

impl Action for TxnOutcome {
    type TState = State;

    fn execute(&self, _state: &mut State) {
        panic!("transaction outcomes are checked through the HTTP route")
    }
}

impl HttpRun for TxnOutcome {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let id = if action.malformed {
                "concurrent".to_owned()
            } else {
                action.id(state)?.to_string()
            };
            let (status, body) = state
                .http_as(action.common.node(), identity, |client| async move {
                    client.tx_outcome(&id).await
                })?
                .map_err(|error| Failure::operation(error.to_string()))?;
            if let Some(expected) = action.expected_status {
                if status != expected {
                    return Err(Failure::operation(format!("HTTP {status}: {body}")));
                }
            }
            if action.expected_error.is_some() {
                return Err(Failure::operation(format!("HTTP {status}: {body}")));
            }
            Ok(body)
        });
    }
}

impl CliRun for TxnOutcome {
    fn execute_cli(&self, state: &mut State) {
        self.execute_http(state)
    }
}

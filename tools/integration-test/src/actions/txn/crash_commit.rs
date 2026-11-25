use crate::client_type::ClientType;
use crate::crash::CrashPoint;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Commits a transaction on a node whose process kills itself with SIGKILL
/// at `point` in the commit. The commit must go unanswered and the process
/// must die of SIGKILL; the node then stays stopped, its data kept for a
/// later `Restart`. The node must be a Rust node in a process of its own
/// (`NewNode { own_process }`), reached by rust-http or cli.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashCommit {
    pub txn: usize,
    pub point: CrashPoint,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for CrashCommit {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run_mut(&self, state: &mut State) -> Result<Value, Failure> {
        let name = self.common.node();
        let identity = self.common.client_identity(Self::name())?;
        let id = state
            .txns
            .get(self.txn)
            .ok_or_else(|| Failure::operation(format!("no transaction {}", self.txn)))?
            .to_string();
        let process = state.node(name)?.process().cloned().ok_or_else(|| {
            Failure::unsupported(format!(
                "{} on node {name}: it is not a Rust node in a process of its own (NewNode \
                 ownProcess)",
                Self::name()
            ))
        })?;
        process.arm(self.point).map_err(Failure::operation)?;
        let answer = match state.client_type() {
            ClientType::RustHttp => state
                .http_as(name, identity, |client| async move {
                    client.tx_commit(&id).await
                })?
                .map_err(|e| e.to_string()),
            ClientType::Cli => state
                .defra_client_as(name, None, identity, &["tx", "commit", &id])
                .map(drop),
            client => {
                return Err(Failure::unsupported(format!(
                    "{} under {client:?}",
                    Self::name()
                )))
            }
        };
        if answer.is_ok() {
            return Err(Failure::operation(format!(
                "the commit was answered success: the node did not crash at {:?}",
                self.point
            )));
        }
        state.kill_node(name, true)?;
        Ok(json!({}))
    }
}

impl Action for CrashCommit {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

use crate::client_type::ClientType;
use crate::nodes::ProcessBinary;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Purges a node: its store is wiped and it starts again empty, as Go's
/// `PurgeAndRestart`. `Node::purge_as` in process, `POST /purge` over HTTP,
/// `client purge --force` through the CLI; a Rust node in this process is
/// then wiped and built again by the test, as its embedding application
/// would. A node in a process of its own restarts itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Purge {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for Purge {
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
        self.common.plain(Self::name())?;
        let name = self.common.node();
        let node = state.node(name)?;
        if let Some(process) = node.process().cloned() {
            let seen = process
                .ready_count()
                .map_err(|e| Failure::operation(e.to_string()))?;
            self.request(state)?;
            let url = process
                .wait_restarted(seen)
                .map_err(|e| Failure::operation(e.to_string()))?;
            if node.spec.process == Some(ProcessBinary::Rust) && node.url.as_deref() != Some(&url) {
                return Err(Failure::operation(format!(
                    "the purged node moved from {:?} to {url}",
                    node.url
                )));
            }
            state.replace_running(name, node.served_at(url));
            return Ok(json!({}));
        }
        if node.core().is_none() {
            return Err(Failure::operation(format!(
                "node {name} has no core to wipe"
            )));
        }
        self.request(state)?;
        // The wipe needs the last handle to the node, as its embedder's would.
        let spec = node.spec.clone();
        drop(node);
        let core = state
            .take_running(name)
            .and_then(|node| node.core())
            .ok_or_else(|| Failure::operation(format!("node {name} is not running")))?;
        state
            .block_on(core.shutdown_and_wipe())
            .map_err(|e| Failure::operation(e.to_string()))?;
        state.start_node(name, spec);
        Ok(json!({}))
    }
}

impl Purge {
    /// Asks node's purge through the test's client.
    fn request(&self, state: &State) -> Result<(), Failure> {
        let name = self.common.node();
        let in_process = state.node(name)?.process().is_none();
        match state.client_type() {
            ClientType::InProcess if in_process => {
                let core = state.node_on(name)?;
                state
                    .block_on(core.purge_as(&state.context(None)))
                    .map_err(|e| Failure::operation(e.to_string()))
            }
            ClientType::Cli => state
                .defra_client_as(name, None, None, &["purge", "--force"])
                .map(drop)
                .map_err(Failure::operation),
            _ => state
                .http_as(name, None, |client| async move { client.purge().await })?
                .map(drop)
                .map_err(|e| Failure::operation(e.to_string())),
        }
    }
}

impl Action for Purge {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

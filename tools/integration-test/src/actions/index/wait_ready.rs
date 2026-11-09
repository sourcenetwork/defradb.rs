use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use testo::prelude::*;

/// How long a build may take before the wait gives up, as in Go's harness.
const BUILD_TIMEOUT: Duration = Duration::from_secs(10);

/// Waits for an index to be built, or for the build gate to block or open.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForIndexReady {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_count: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blocked: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub release_gate: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for WaitForIndexReady {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    /// Opens the node's build gate, waits for a build to block at it or for
    /// builds of `expected_count` collections to reach it, or waits until no
    /// index of `collection` is building, as Go's harness does.
    fn run(&self, state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let node = state.node(self.common.node())?;
        if self.release_gate || self.blocked || self.expected_count.is_some() {
            let gate = node.spec.index_gate.clone().ok_or_else(|| {
                Failure::operation("no index build gate: NewNode { indexGate } did not run".into())
            })?;
            if self.release_gate {
                gate.release().map_err(Failure::operation)?;
            }
            if self.blocked {
                gate.wait_blocked().map_err(Failure::operation)?;
            }
            if let Some(count) = self.expected_count {
                gate.wait_collections(count.max(0) as usize)
                    .map_err(Failure::operation)?;
            }
        }
        let Some(collection) = &self.collection else {
            return Ok(json!({}));
        };
        let core = node.core().ok_or_else(|| {
            Failure::unsupported(format!("{} on a node in another process", Self::name()))
        })?;
        let deadline = Instant::now() + BUILD_TIMEOUT;
        loop {
            let listing = state
                .block_on(core.list_indexes_as(&state.context(None), collection))
                .map_err(|e| Failure::operation(e.to_string()))?;
            let listing =
                serde_json::to_value(listing).map_err(|e| Failure::operation(e.to_string()))?;
            let still_building = listing
                .as_array()
                .is_some_and(|all| all.iter().any(super::new::building));
            if !still_building {
                return Ok(json!({}));
            }
            if Instant::now() > deadline {
                return Err(Failure::operation(format!(
                    "{}: an index of {collection} was still building after {BUILD_TIMEOUT:?}",
                    Self::name()
                )));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Action for WaitForIndexReady {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

use std::collections::BTreeMap;

use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Checks counters in a Rust node's `p2p status`: each counter named in
/// `at_most` is no more than its limit, and each named in `at_least` no less.
/// A counter named in `since` is checked as its rise past the value captured
/// under that name by an earlier `SyncCounters` (the result is the whole
/// status, so `capture` takes `/<counter>`). The node is read directly,
/// whichever client the test runs through; a Go node has no such status.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncCounters {
    #[serde(default)]
    pub at_most: BTreeMap<String, u64>,
    #[serde(default)]
    pub at_least: BTreeMap<String, u64>,
    #[serde(default)]
    pub since: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for SyncCounters {
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
        let test_node = state.node(self.common.node())?;
        let node = test_node
            .embedded()
            .ok_or_else(|| Failure::unsupported("SyncCounters on a Go node"))?;
        let p2p = node
            .p2p()
            .ok_or_else(|| Failure::operation("the node has no P2P".to_owned()))?;
        let status = state
            .block_on(p2p.sync_status())
            .map_err(|e| Failure::operation(e.to_string()))?;
        let mut seen = serde_json::Map::new();
        for (counter, limit) in &self.at_most {
            let value = self.counter(state, &status, counter)?;
            if value > *limit {
                return Err(Failure::operation(format!(
                    "{counter} is {value}, more than {limit}"
                )));
            }
            seen.insert(counter.clone(), json!(value));
        }
        for (counter, limit) in &self.at_least {
            let value = self.counter(state, &status, counter)?;
            if value < *limit {
                return Err(Failure::operation(format!(
                    "{counter} is {value}, less than {limit}"
                )));
            }
            seen.insert(counter.clone(), json!(value));
        }
        println!(
            "{} sync counters: {}",
            self.common.node(),
            Value::Object(seen)
        );
        Ok(status)
    }
}

impl SyncCounters {
    /// `counter`'s value, less its captured baseline when `since` names one.
    fn counter(&self, state: &State, status: &Value, counter: &str) -> Result<u64, Failure> {
        let value = status[counter]
            .as_u64()
            .ok_or_else(|| Failure::operation(format!("status has no counter {counter}")))?;
        let Some(capture) = self.since.get(counter) else {
            return Ok(value);
        };
        let base = state
            .captures
            .get(capture)
            .and_then(Value::as_u64)
            .ok_or_else(|| Failure::operation(format!("no counter captured as {capture}")))?;
        // A counter below its baseline was reset (a restarted node), so a
        // rise measured from that baseline would be meaningless.
        value.checked_sub(base).ok_or_else(|| {
            Failure::operation(format!(
                "{counter} is {value}, below its captured {capture} of {base}"
            ))
        })
    }
}

impl Action for SyncCounters {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

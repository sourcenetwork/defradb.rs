use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Checks counters in a Rust node's `p2p status`: each counter named in
/// `at_most` is no more than its limit, and each named in `at_least` no less.
/// A name may use `/` to descend into a nested object.
/// A counter named in `since` is checked as its rise past the value captured
/// under that name by an earlier `SyncCounters` (the result is the whole
/// status, so `capture` takes `/<counter>`). With `eventually`, the status is
/// read again until every bound holds or the timeout passes. The node is read directly,
/// whichever client the test runs through; a Go node has no such status.
///
/// `slotPeer` names a capture holding a peer ID: then every slot in
/// `pending_dag_fetch_slots.roots` must name that peer as its `peer`, and
/// there must be at least one such slot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncCounters {
    #[serde(default)]
    pub at_most: BTreeMap<String, u64>,
    #[serde(default)]
    pub at_least: BTreeMap<String, u64>,
    #[serde(default)]
    pub since: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_peer: Option<String>,
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
        let read = || {
            state
                .block_on(p2p.sync_status())
                .map_err(|e| Failure::operation(e.to_string()))
        };
        let Some(timing) = &self.common.eventually else {
            return self.check(state, read()?);
        };
        let timeout = timing.timeout_millis.unwrap_or(EVENTUALLY_TIMEOUT_MILLIS);
        let interval = Duration::from_millis(timing.interval_millis.unwrap_or(100));
        let deadline = Instant::now() + Duration::from_millis(timeout);
        loop {
            match self.check(state, read()?) {
                Err(_) if Instant::now() < deadline => std::thread::sleep(interval),
                outcome => return outcome,
            }
        }
    }
}

/// How long an `eventually` read waits for its bounds to hold by default.
const EVENTUALLY_TIMEOUT_MILLIS: u64 = 20_000;

impl SyncCounters {
    /// `status`, once every bound holds in it.
    fn check(&self, state: &State, status: Value) -> Result<Value, Failure> {
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
        if let Some(capture) = &self.slot_peer {
            let peer = state
                .captures
                .get(capture)
                .and_then(Value::as_str)
                .ok_or_else(|| Failure::operation(format!("no peer captured as {capture}")))?;
            let roots = status
                .pointer("/pending_dag_fetch_slots/roots")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    Failure::operation("status has no pending_dag_fetch_slots/roots".to_owned())
                })?;
            if roots.is_empty() {
                return Err(Failure::operation(
                    "no pending-DAG fetch slot is held".to_owned(),
                ));
            }
            if let Some(root) = roots
                .iter()
                .find(|root| root["peer"].as_str() != Some(peer))
            {
                return Err(Failure::operation(format!(
                    "a held slot names {:?} instead of the pushing peer {peer}",
                    root["peer"]
                )));
            }
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
        let value = status
            .pointer(&format!("/{counter}"))
            .and_then(Value::as_u64)
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

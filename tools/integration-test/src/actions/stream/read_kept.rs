use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

use super::in_process;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// Reads a kept stream to its end. Fails with `stream ended: <why>` when the
/// serving side cut it, and answers `{"rest": <text>}` when it finished.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadKeptStream {
    pub stream: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ReadKeptStream {
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
        in_process(state, self.common.node(), Self::name())?;
        let mut kept = state.streams.take(&self.stream)?;
        let read = state
            .block_on(workers::timeout(
                Duration::from_secs(10),
                kept.recv.read_to_end(1 << 20),
            ))
            .map_err(|_| Failure::operation("the stream is still open".into()))?
            .map_err(|e| Failure::operation(format!("stream ended: {e}")))?;
        Ok(json!({ "rest": String::from_utf8_lossy(&read) }))
    }
}

impl Action for ReadKeptStream {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

use crate::actions::peer::support::MERGE_EVENT;
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `peer.awaitSESync` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForSESync {
    #[serde(rename = "docIDs")]
    pub doc_ids: Vec<Text>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for WaitForSESync {
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
        if self.doc_ids.is_empty() {
            return Err(Failure::unsupported(format!(
                "{} with no docIDs: the harness does not track every document",
                Self::name()
            )));
        }
        let events = state
            .node(self.common.node())?
            .events()
            .map_err(Failure::unsupported)?;
        let doc_ids: Vec<String> = self
            .doc_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        state
            .block_on(events.se_synced(&doc_ids, MERGE_EVENT))
            .map_err(Failure::operation)?;
        Ok(json!({}))
    }
}

impl Action for WaitForSESync {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

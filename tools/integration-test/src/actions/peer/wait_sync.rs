use crate::actions::peer::support::{MERGE_EVENT, UPDATE_EVENT};
use crate::scenario::Text;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Waits for a node to receive documents from its peers: an update, several, the changed fields, or a block merge.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForSync {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "docID")]
    pub doc_id: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "docIDs")]
    pub doc_ids: Option<Vec<Text>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "blockCID")]
    pub cid: Option<Text>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub updated_fields: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for WaitForSync {
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
        if self.updated_fields {
            return Err(Failure::unsupported("WaitForSync argument updatedFields"));
        }
        let node = state.node(self.common.node())?;
        let events = node.events().map_err(Failure::unsupported)?;
        if let (Some(doc_id), Some(cid)) = (&self.doc_id, &self.cid) {
            state
                .block_on(events.merged(doc_id.as_str(), cid.as_str(), MERGE_EVENT))
                .map_err(Failure::operation)?;
            return Ok(json!({}));
        }
        let keys: Vec<&str> = match &self.doc_id {
            Some(doc_id) => vec![doc_id.as_str()],
            None => self
                .doc_ids
                .iter()
                .flatten()
                .map(|id| id.as_str())
                .collect(),
        };
        let cids = state
            .block_on(events.next_updates(&keys, UPDATE_EVENT))
            .map_err(Failure::operation)?;
        if self.doc_id.is_some() {
            return Ok(json!({ "cid": cids.first() }));
        }
        Ok(json!({ "cids": cids }))
    }
}

impl Action for WaitForSync {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

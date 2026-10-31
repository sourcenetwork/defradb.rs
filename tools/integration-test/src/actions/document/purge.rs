use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `document.purge` op: `DbTransactionRegistry::purge_by_doc_ids_as`,
/// through the node's core. It has no HTTP or CLI route, so only an
/// in-process node runs it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeDocs {
    pub collection: String,
    #[serde(rename = "docIDs")]
    pub doc_ids: Vec<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    /// How many documents the purge must report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for PurgeDocs {
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
        let Some(node) = state.node(self.common.node())?.node.clone() else {
            return Err(Failure::unsupported(
                "purge on a node without its core: it has no HTTP or CLI route",
            ));
        };
        let doc_ids = self
            .doc_ids
            .iter()
            .map(|id| document::DocID::from_string(id.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Failure::operation(e.to_string()))?;
        let purged = state
            .block_on(node.txn_registry().purge_by_doc_ids_as(
                &state.context(self.txn),
                &self.collection,
                &doc_ids,
            ))
            .map_err(|e| Failure::operation(e.to_string()))?;
        if let Some(expected) = self.expected_count {
            if purged != expected {
                return Err(Failure::operation(format!(
                    "expected {expected} purged documents; got {purged}"
                )));
            }
        }
        Ok(json!({}))
    }
}

impl Action for PurgeDocs {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

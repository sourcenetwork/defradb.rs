use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use query::documents::AddOptions;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `document.saveMany` op: `Node::save_docs`, through the node's core.
/// It has no HTTP or CLI route, so only an in-process node runs it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDocs {
    pub collection: String,
    /// The documents, each updated by the `_docID` it carries or added.
    pub docs: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for SaveDocs {
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
                "saveMany on a Go node: Go refuses it over HTTP",
            ));
        };
        state
            .block_on(node.save_docs(
                &state.context(self.txn),
                &self.collection,
                &self.docs,
                &AddOptions::default(),
            ))
            .map(|doc_ids| json!({ "docIDs": doc_ids }))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for SaveDocs {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

use crate::actions::peer::support::{ambient, failed};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `peer.documents.add` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDocumentSubscription {
    #[serde(rename = "docIDs")]
    pub doc_ids: Vec<Text>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddDocumentSubscription {
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
        let ids = self.ids();
        let node = state.node_on(self.common.node())?;
        state
            .block_on(node.add_p2p_documents_as(&ambient(), &ids))
            .map_err(failed)?;
        Ok(json!({}))
    }
}

impl Action for AddDocumentSubscription {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for AddDocumentSubscription {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let ids = action.ids();
            state
                .http_on(action.common.node(), |client| async move {
                    client.add_p2p_documents(&ids).await
                })?
                .map_err(failed)?;
            Ok(json!({}))
        });
    }
}

impl CliRun for AddDocumentSubscription {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let ids = action.ids();
            state
                .defra_client_on(
                    action.common.node(),
                    None,
                    &[
                        &["p2p", "document", "add"],
                        ids.iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .as_slice(),
                    ]
                    .concat(),
                )
                .map_err(Failure::operation)?;
            Ok(json!({}))
        });
    }
}
impl AddDocumentSubscription {
    fn ids(&self) -> Vec<String> {
        self.doc_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect()
    }
}

use crate::actions::peer::support::{ambient, failed};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `peer.collections.list` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListP2PCollections {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ListP2PCollections {
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

        let node = state.node_on(self.common.node())?;
        let listed = state
            .block_on(node.list_p2p_collections_as(&ambient()))
            .map_err(failed)?;
        Ok(json!({ "collections": listed }))
    }
}

impl Action for ListP2PCollections {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for ListP2PCollections {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;

            let listed = state
                .http_on(action.common.node(), |client| async move {
                    client.list_p2p_collections().await
                })?
                .map_err(failed)?;
            Ok(json!({ "collections": listed }))
        });
    }
}

impl CliRun for ListP2PCollections {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;

            let printed = state
                .defra_client_on(action.common.node(), None, &["p2p", "collection", "list"])
                .map_err(Failure::operation)?;
            Ok(json!({ "collections": printed }))
        });
    }
}

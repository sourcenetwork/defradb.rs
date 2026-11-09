use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `collection.syncVersions` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncCollectionVersions {
    #[serde(rename = "versionIDs")]
    pub version_ids: Vec<Text>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for SyncCollectionVersions {
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
        let identity = self.common.client_identity(Self::name())?;
        state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .sync_collection_versions_as(&state.context_as(None, identity), &self.ids()),
            )
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for SyncCollectionVersions {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl SyncCollectionVersions {
    fn ids(&self) -> Vec<String> {
        self.version_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect()
    }
}

impl HttpRun for SyncCollectionVersions {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let ids = action.ids();
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.p2p_collection_sync_versions(&ids).await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for SyncCollectionVersions {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let ids = action.ids();
            let mut args = vec!["p2p", "collection", "sync-versions"];
            args.extend(ids.iter().map(String::as_str));
            state
                .defra_client_as(action.common.node(), None, identity, &args)
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}

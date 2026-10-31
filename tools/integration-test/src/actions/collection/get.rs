use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Lists the collection versions a node holds, optionally filtered.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCollections {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub get_inactive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "versionID")]
    pub version_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "collectionID")]
    pub collection_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub filtered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for GetCollections {
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
        let collections = state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .get_collections_as(&state.context(self.txn), &self.selector()),
            )
            .map_err(|e| Failure::operation(e.to_string()))?;
        Ok(json!({ "collections": collections }))
    }
}

impl GetCollections {
    fn selector(&self) -> schema::CollectionSelector {
        schema::CollectionSelector {
            names: self.collection_name.clone().map(|name| vec![name]),
            version_id: self.version_id.clone(),
            collection_id: self.collection_id.clone(),
            get_inactive: self.get_inactive.unwrap_or_default(),
        }
    }
}

impl Action for GetCollections {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for GetCollections {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            action.common.plain(Self::name())?;
            let (txn, selector) = (state.txn_id(action.txn), action.selector());
            let collections = state
                .http_on(action.common.node(), |client| async move {
                    client.get_collections(txn.as_deref(), &selector).await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            Ok(json!({ "collections": collections }))
        });
    }
}

impl CliRun for GetCollections {
    /// The Rust CLI prints no collection descriptions (its `collection
    /// describe` is GraphQL introspection), so the CLI client reads them from
    /// the same node over its HTTP API, which a CLI node serves.
    fn execute_cli(&self, state: &mut State) {
        HttpRun::execute_http(self, state);
    }
}

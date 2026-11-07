use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `collection.syncBranchable` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncBranchableCollection {
    pub collection_index: u64,
    pub collections: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_deadline: Option<bool>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for SyncBranchableCollection {
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
        let node = state.node_on(self.common.node())?;
        let context = state.context_as(None, identity);
        let collections = state
            .block_on(node.get_collections_as(&context, &self.selector()?))
            .map_err(|e| Failure::operation(e.to_string()))?;
        let id = Self::collection_id(&collections)?;
        state
            .block_on(node.sync_branchable_collection_as(&context, &id))
            .map(|()| json!({}))
            .map_err(|e| Failure::operation(e.to_string()))
    }
}

impl Action for SyncBranchableCollection {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl SyncBranchableCollection {
    /// The collection the action names, Go's `collections[collectionIndex]`.
    fn selector(&self) -> Result<schema::CollectionSelector, Failure> {
        let name = usize::try_from(self.collection_index)
            .ok()
            .and_then(|index| self.collections.get(index))
            .ok_or_else(|| {
                Failure::operation(format!(
                    "no collection {} in {:?}",
                    self.collection_index, self.collections
                ))
            })?;
        Ok(schema::CollectionSelector {
            names: Some(vec![name.clone()]),
            ..Default::default()
        })
    }

    fn collection_id(collections: &[schema::CollectionVersion]) -> Result<String, Failure> {
        collections
            .first()
            .map(|collection| collection.collection_id.clone())
            .ok_or_else(|| Failure::operation("the collection to sync does not exist".to_owned()))
    }
}

impl HttpRun for SyncBranchableCollection {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let selector = action.selector()?;
            let collections = state
                .http_as(action.common.node(), identity, |client| async move {
                    client.get_collections(None, &selector).await
                })?
                .map_err(|e| Failure::operation(e.to_string()))?;
            let id = Self::collection_id(&collections)?;
            state
                .http_as(action.common.node(), identity, |client| async move {
                    client.p2p_collection_sync_branchable(&id).await
                })?
                .map(|()| json!({}))
                .map_err(|e| Failure::operation(e.to_string()))
        });
    }
}

impl CliRun for SyncBranchableCollection {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            let identity = action.common.client_identity(Self::name())?;
            let names = action.selector()?.names.unwrap_or_default();
            let name = names.first().map(String::as_str).unwrap_or_default();
            let described = state
                .defra_client_as(
                    action.common.node(),
                    None,
                    identity,
                    &["collection", "describe", "--collection-name", name],
                )
                .map_err(Failure::operation)?;
            let collections: Vec<schema::CollectionVersion> = serde_json::from_value(described)
                .map_err(|e| Failure::operation(format!("collection describe output: {e}")))?;
            let id = Self::collection_id(&collections)?;
            state
                .defra_client_as(
                    action.common.node(),
                    None,
                    identity,
                    &["p2p", "collection", "sync-branchable", id.as_str()],
                )
                .map(|_| json!({}))
                .map_err(Failure::operation)
        });
    }
}

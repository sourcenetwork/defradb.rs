use crate::client_type::ClientType;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `encryptedIndex.listAll` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAllEncryptedIndexes {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ListAllEncryptedIndexes {
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
        let node = self.common.node();
        match state.client_type() {
            ClientType::InProcess => {}
            ClientType::RustHttp | ClientType::GoHttp => {
                let all = state
                    .http_as(node, identity, |client| async move {
                        client.encrypted_index_list_all().await
                    })?
                    .map_err(|e| Failure::operation(e.to_string()))?;
                return Ok(json!({ "indexes": all }));
            }
            ClientType::Cli => {
                let all = state
                    .defra_client_as(node, None, identity, &["encrypted-index", "list"])
                    .map_err(Failure::operation)?;
                return Ok(json!({ "indexes": all }));
            }
        }
        let all = state
            .block_on(
                state
                    .node_on(self.common.node())?
                    .list_all_encrypted_indexes_as(&state.context_as(None, identity)),
            )
            .map_err(|e| Failure::operation(e.to_string()))?;
        let by_collection: serde_json::Map<String, Value> = all
            .into_iter()
            .map(|(name, indexes)| (name, json!(indexes)))
            .collect();
        Ok(json!({ "indexes": by_collection }))
    }
}

impl Action for ListAllEncryptedIndexes {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

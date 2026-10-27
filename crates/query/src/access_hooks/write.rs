use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;
use identity::Did;
use rapidhash::RapidHashMap;
use schema::CollectionVersion;
use serde_json::Value as JsonValue;

use crate::mapper::{InputOp, MutationType};

/// A mutation a client of this node asks to run, before any block is built.
pub struct WriteRequest<'a> {
    pub identity: Option<&'a Did>,
    pub collection: &'a CollectionVersion,
    pub kind: MutationType,
    /// Targets of an update, upsert or delete, resolved from IDs or filter.
    /// Empty for a create, whose document IDs derive from blocks not yet built.
    pub doc_ids: &'a [String],
    pub create_input: &'a [RapidHashMap<String, JsonValue>],
    pub update_input: &'a RapidHashMap<String, JsonValue>,
    /// Explicit CRDT ops of an update, keyed by field.
    pub update_ops: &'a RapidHashMap<String, InputOp>,
}

/// Refuses local mutations early, so a node does not author blocks its own
/// merge validators or its peers would refuse.
///
/// Called for GraphQL mutations before ACP checks and before any block is
/// built. It is node-local and may read anything the node knows. `Err`
/// refuses the mutation with that message.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait WriteValidator: MaybeSendSync {
    async fn validate_write(&self, request: &WriteRequest<'_>) -> Result<(), String>;
}

/// Exact, serializable validator request carried across transaction retries.
/// The full collection version is included because validators may inspect
/// structural fields that are not represented by `version_id` alone.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct WriteValidationInput {
    pub validator: usize,
    pub identity: Option<String>,
    #[serde(with = "super::collection_input")]
    pub collection: CollectionVersion,
    pub kind: String,
    pub doc_ids: Vec<String>,
    pub create_input: Vec<RapidHashMap<String, JsonValue>>,
    pub update_input: RapidHashMap<String, JsonValue>,
    pub update_ops: RapidHashMap<String, InputOp>,
}

impl WriteValidationInput {
    #[cfg(test)]
    pub(crate) fn prepared_request(
        &self,
    ) -> serde_json::Result<crate::prepared::PreparedInputRequest> {
        // Normalize map ordering so a retry with the same semantic request
        // hits the memo even when a RapidHashMap iterates differently.
        let canonical = serde_json::to_value(self)?;
        let request = serde_json::to_vec(&canonical)?;
        Ok(crate::prepared::PreparedInputRequest::new(
            crate::prepared::PreparedInputKind::WriteValidation,
            request,
        ))
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[test]
    fn prepared_write_validation_key_includes_full_collection_schema() {
        let mut first = WriteValidationInput {
            validator: 1,
            identity: None,
            collection: CollectionVersion::new("Users", "same-version", "users", vec![]),
            kind: "update".to_string(),
            doc_ids: vec!["doc-1".to_string()],
            create_input: vec![],
            update_input: RapidHashMap::default(),
            update_ops: RapidHashMap::default(),
        };
        first.collection.root_id = 42;
        let first_key = first.prepared_request().unwrap();
        let restored: WriteValidationInput =
            serde_json::from_slice(first_key.exact_request()).unwrap();
        assert_eq!(restored.collection.root_id, 42);
        first.collection.root_id = 43;
        assert_ne!(first_key, first.prepared_request().unwrap());
        first.collection.root_id = 42;
        first.collection.policy = Some(schema::PolicyDescription::new("policy", "Users"));
        let changed_schema_key = first.prepared_request().unwrap();

        assert_ne!(first_key, changed_schema_key);
    }
}

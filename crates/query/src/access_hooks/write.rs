use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;
use identity::Did;
use rapidhash::RapidHashMap;
use schema::CollectionVersion;
use serde_json::Value as JsonValue;

use crate::mapper::MutationType;

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

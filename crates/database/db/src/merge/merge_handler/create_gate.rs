//! The relationships a replicated document carries through its
//! `@policy(relation:)` fields, derived from what a composite merged, for its
//! registration on the receiving node (see `acp_merge_handler`).

use document::NormalValue;
use rapidhash::RapidHashMap;
use schema::CollectionVersion;
use storage::corekv::Store;

use super::DbMergeHandler;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// The relationships `fields`, the values a composite of a document in
    /// `schema` merged, bind: none when the collection binds no field, when
    /// the composite merged no bound value (only a genesis carries the
    /// Immutable block), or when the policy cannot be read. A registration
    /// sent without them is refused by a provider that gates the
    /// collection's creates, so nothing is widened.
    pub(super) async fn merged_relationships(
        &self,
        schema: &CollectionVersion,
        fields: &RapidHashMap<String, NormalValue>,
    ) -> Vec<acp::DerivedRelationship> {
        let (Some(described), Some(policies)) =
            (schema.policy.as_ref(), self.db.dac_policies.get())
        else {
            return Vec::new();
        };
        if !schema
            .policy_bindings()
            .any(|binding| fields.contains_key(&binding.id_field.name))
        {
            return Vec::new();
        }
        let policy = match policies.get(&described.id).await {
            Ok(policy) => policy,
            Err(error) => {
                tracing::warn!(%error, collection = %schema.name, "Policy of a replicated document is unreadable; it is registered without its derived relationships");
                return Vec::new();
            }
        };
        let value = |field: &str| match fields.get(field) {
            Some(NormalValue::String(value)) => Some(value.clone()),
            _ => None,
        };
        match crate::collection::create_gate::create_gate_of(schema, value, policy.as_ref()) {
            Ok(gate) => gate.derived,
            Err(error) => {
                tracing::warn!(%error, collection = %schema.name, "Relationships of a replicated document cannot be derived; it is registered without them");
                Vec::new()
            }
        }
    }
}

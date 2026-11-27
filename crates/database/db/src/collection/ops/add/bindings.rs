//! The add-time check of `@policy(relation:)` bindings.

use super::*;

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// Refuse a binding the collection cannot honour: see
    /// [`schema::check_policy_bindings`] for what the schema must say, and
    /// [`acp::validate_relation_binding`] for what the policy must say. The
    /// policy is read only when DAC is available, as [`Self::check_policies`]
    /// reads it.
    pub(super) async fn check_policy_bindings(
        &self,
        collections: &[CollectionVersion],
    ) -> std::result::Result<(), AddCollectionError> {
        if collections
            .iter()
            .all(|c| c.policy_bindings().next().is_none())
        {
            return Ok(());
        }
        let mut known = collections.to_vec();
        for name in self
            .list_collections()
            .map_err(|e| AddCollectionError::Storage(e.to_string()))?
        {
            if collections.iter().any(|c| c.name == name) {
                continue;
            }
            if let Some(existing) = self
                .get_collection(&name)
                .map_err(|e| AddCollectionError::Storage(e.to_string()))?
            {
                known.push(existing.schema().clone());
            }
        }
        for collection in collections {
            let bound = schema::check_policy_bindings(collection, &known)
                .map_err(|e| AddCollectionError::Validation(e.to_string()))?;
            let (Some(policies), Some(policy)) = (self.dac_policies(), collection.policy.as_ref())
            else {
                continue;
            };
            if bound.is_empty() {
                continue;
            }
            let stored = policies
                .get(&policy.id)
                .await
                .map_err(|e| AddCollectionError::Policy(e.to_string()))?
                .ok_or_else(|| {
                    AddCollectionError::Policy(format!(
                        "policyID specified does not exist with acp. PolicyID: {}",
                        policy.id
                    ))
                })?;
            for relation in bound {
                let target_resource = relation
                    .target
                    .policy
                    .as_ref()
                    .map(|p| p.resource_name.as_str())
                    .unwrap_or_default();
                acp::validate_relation_binding(
                    &stored,
                    &policy.resource_name,
                    relation.binding.policy_relation,
                    target_resource,
                )
                .map_err(|reason| {
                    AddCollectionError::Policy(format!(
                        "invalid @policy(relation:) on field {}.{}: {reason}",
                        collection.name,
                        relation.binding.declared_name()
                    ))
                })?;
            }
        }
        Ok(())
    }
}

use async_trait::async_trait;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::did::Did;

use super::traits::ZanzibarStore;
use crate::error::Result;
use crate::types::{ObjectRef, Policy, Relationship, Subject};

type RelationshipMap = HopscotchMap<String, Relationship, RandomState>;

pub struct MemoryZanzibarStore {
    policies: HopscotchMap<String, Policy, RandomState>,
    relationships: HopscotchMap<String, Arc<RelationshipMap>, RandomState>,
    policy_counter: AtomicU64,
}

impl MemoryZanzibarStore {
    pub fn new() -> Self {
        Self {
            policies: HopscotchMap::with_hasher(RandomState::default()),
            relationships: HopscotchMap::with_hasher(RandomState::default()),
            policy_counter: AtomicU64::new(0),
        }
    }
}

impl Default for MemoryZanzibarStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ZanzibarStore for MemoryZanzibarStore {
    async fn store_policy(&self, policy: &Policy) -> Result<()> {
        self.policies.insert(policy.id.clone(), policy.clone());
        Ok(())
    }

    async fn get_policy(&self, policy_id: &str) -> Result<Option<Policy>> {
        Ok(self.policies.get(policy_id))
    }

    async fn next_policy_counter(&self) -> Result<u64> {
        Ok(self.policy_counter.fetch_add(1, Ordering::SeqCst) + 1)
    }

    async fn list_policies(&self) -> Result<Vec<Policy>> {
        Ok(self.policies.values().collect())
    }

    async fn delete_policy(&self, policy_id: &str) -> Result<bool> {
        let removed = self.policies.remove(policy_id).is_some();
        if removed {
            self.relationships.remove(policy_id);
        }
        Ok(removed)
    }

    async fn store_relationship(&self, policy_id: &str, rel: &Relationship) -> Result<()> {
        let key = rel.storage_key();
        let rels = self.relationships.get_or_insert(
            policy_id.to_string(),
            Arc::new(HopscotchMap::with_hasher(RandomState::default())),
        );
        rels.insert(key, rel.clone());
        Ok(())
    }

    async fn delete_relationship(&self, policy_id: &str, rel: &Relationship) -> Result<bool> {
        let key = rel.storage_key();
        if let Some(rels) = self.relationships.get(policy_id) {
            return Ok(rels.remove(&key).is_some());
        }
        Ok(false)
    }

    async fn has_relationship(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Subject,
    ) -> Result<bool> {
        let rel = Relationship::new(resource, object_id, relation, subject.clone());
        let key = rel.storage_key();

        if let Some(rels) = self.relationships.get(policy_id) {
            return Ok(rels.contains_key(&key));
        }
        Ok(false)
    }

    async fn check_permission_direct(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Did,
    ) -> Result<bool> {
        let direct = Relationship::with_entity(resource, object_id, relation, subject.clone());
        let direct_key = direct.storage_key();

        let wildcard = Relationship::new(resource, object_id, relation, Subject::Wildcard);
        let wildcard_key = wildcard.storage_key();

        if let Some(rels) = self.relationships.get(policy_id) {
            if rels.contains_key(&direct_key) || rels.contains_key(&wildcard_key) {
                return Ok(true);
            }

            let prefix = Relationship::relation_prefix(resource, object_id, relation);
            for (key, rel) in rels.iter() {
                if key.starts_with(&prefix) && rel.subject.is_typed_wildcard() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    async fn get_relation_subjects(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
    ) -> Result<Vec<Subject>> {
        let prefix = Relationship::relation_prefix(resource, object_id, relation);

        if let Some(rels) = self.relationships.get(policy_id) {
            let subjects: Vec<_> = rels
                .iter()
                .filter(|(k, _)| k.starts_with(&prefix))
                .map(|(_, v)| v.subject.clone())
                .collect();
            return Ok(subjects);
        }
        Ok(Vec::new())
    }

    async fn get_relation_targets(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
    ) -> Result<Vec<ObjectRef>> {
        let prefix = Relationship::relation_prefix(resource, object_id, relation);

        if let Some(rels) = self.relationships.get(policy_id) {
            let targets: Vec<_> = rels
                .iter()
                .filter(|(k, _)| k.starts_with(&prefix))
                .filter_map(|(_, v)| match &v.subject {
                    Subject::EntitySet {
                        resource,
                        object_id,
                        ..
                    } => Some(ObjectRef::new(resource, object_id)),
                    _ => None,
                })
                .collect();
            return Ok(targets);
        }
        Ok(Vec::new())
    }

    async fn delete_object_relationships(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<()> {
        let prefix = Relationship::object_prefix(resource, object_id);

        if let Some(rels) = self.relationships.get(policy_id) {
            let dead: Vec<String> = rels.keys().filter(|k| k.starts_with(&prefix)).collect();
            for key in &dead {
                rels.remove(key);
            }
        }
        Ok(())
    }
}

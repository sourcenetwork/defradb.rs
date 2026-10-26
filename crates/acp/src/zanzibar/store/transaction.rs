//! Zanzibar reads and writes bound to the authorized database transaction.
//! The caller owns commit/discard; no operation opens or commits another transaction.
use datastore::NamespaceView;
use identity::Did;
use storage::corekv::IterOptions;
use zanzibar::error::{Error, Result};
use zanzibar::store::ZanzibarStore;
use zanzibar::types::{ObjectRef, Policy, Relationship, Subject};

pub struct TransactionZanzibarStore {
    view: NamespaceView,
}
const POLICY_PREFIX: &str = "/zanzibar/policy/";
impl TransactionZanzibarStore {
    pub fn new(view: NamespaceView) -> Self {
        Self { view }
    }
    fn policy_key(policy_id: &str) -> String {
        format!("{}{}", POLICY_PREFIX, policy_id)
    }

    fn relationship_key(policy_id: &str, rel: &Relationship) -> String {
        format!("/zanzibar/{}{}", policy_id, rel.storage_key())
    }

    fn relationship_prefix(policy_id: &str, resource: &str, object_id: &str) -> String {
        format!(
            "/zanzibar/{}{}",
            policy_id,
            Relationship::object_prefix(resource, object_id)
        )
    }

    fn relation_prefix(policy_id: &str, resource: &str, object_id: &str, relation: &str) -> String {
        format!(
            "/zanzibar/{}{}",
            policy_id,
            Relationship::relation_prefix(resource, object_id, relation)
        )
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ZanzibarStore for TransactionZanzibarStore {
    fn supports_transactions(&self) -> bool {
        true
    }
    async fn store_policy(&self, policy: &Policy) -> Result<()> {
        let txn = &self.view;

        let key = Self::policy_key(&policy.id);
        let value = serde_json::to_vec(policy)?;
        if let Some(existing) = txn
            .get(key.as_bytes())
            .await
            .map_err(|error| Error::Serialization(error.to_string()))?
        {
            if existing.as_ref() != value.as_slice() {
                return Err(Error::PolicyAlreadyExists(policy.id.clone()));
            }
            return Ok(());
        }

        txn.set(key.as_bytes(), &value)
            .await
            .map_err(|e| Error::Serialization(format!("store_policy: set key {}: {}", key, e)))?;

        Ok(())
    }

    async fn get_policy(&self, policy_id: &str) -> Result<Option<Policy>> {
        let txn = &self.view;

        let key = Self::policy_key(policy_id);

        match txn
            .get(key.as_bytes())
            .await
            .map_err(|e| Error::Serialization(format!("get_policy: get key {}: {}", key, e)))?
        {
            Some(data) => {
                let policy: Policy = serde_json::from_slice(&data)?;
                Ok(Some(policy))
            }
            None => Ok(None),
        }
    }

    async fn has_policy(&self, policy_id: &str) -> Result<bool> {
        self.view
            .has(Self::policy_key(policy_id).as_bytes())
            .await
            .map_err(|e| Error::Serialization(format!("has_policy: {}", e)))
    }

    async fn list_policies(&self) -> Result<Vec<Policy>> {
        let txn = &self.view;

        let iter_opts = IterOptions::new()
            .with_validation(true)
            .with_prefix(POLICY_PREFIX.as_bytes().to_vec());

        let mut iter = txn
            .iterator(iter_opts)
            .await
            .map_err(|e| Error::Serialization(format!("list_policies: create iterator: {}", e)))?;

        let mut policies = Vec::new();
        while let Some(kv) = iter
            .next()
            .await
            .map_err(|e| Error::Serialization(format!("list_policies: iterate: {}", e)))?
        {
            let policy: Policy = serde_json::from_slice(&kv.value)?;
            policies.push(policy);
        }

        Ok(policies)
    }

    async fn next_policy_counter(&self) -> Result<u64> {
        Err(Error::Serialization(
            "policy IDs must be allocated before the mutation transaction".into(),
        ))
    }

    async fn delete_policy(&self, policy_id: &str) -> Result<bool> {
        let txn = &self.view;

        let key = Self::policy_key(policy_id);

        let exists = txn
            .has(key.as_bytes())
            .await
            .map_err(|e| Error::Serialization(e.to_string()))?;

        if exists {
            txn.delete(key.as_bytes())
                .await
                .map_err(|e| Error::Serialization(e.to_string()))?;

            let rel_prefix = format!("/zanzibar/{}/rel/", policy_id);
            let iter_opts = IterOptions::new()
                .with_validation(true)
                .with_prefix(rel_prefix.into_bytes());

            let mut keys_to_delete = Vec::new();
            {
                let mut iter = txn
                    .iterator(iter_opts)
                    .await
                    .map_err(|e| Error::Serialization(e.to_string()))?;

                while let Some(kv) = iter
                    .next()
                    .await
                    .map_err(|e| Error::Serialization(e.to_string()))?
                {
                    keys_to_delete.push(kv.key);
                }
            }

            for key in keys_to_delete {
                txn.delete(&key)
                    .await
                    .map_err(|e| Error::Serialization(e.to_string()))?;
            }
        }

        Ok(exists)
    }

    async fn store_relationship(&self, policy_id: &str, rel: &Relationship) -> Result<()> {
        let txn = &self.view;

        let key = Self::relationship_key(policy_id, rel);
        let value = serde_json::to_vec(rel)?;

        txn.set(key.as_bytes(), &value)
            .await
            .map_err(|e| Error::Serialization(e.to_string()))?;

        Ok(())
    }

    async fn delete_relationship(&self, policy_id: &str, rel: &Relationship) -> Result<bool> {
        let txn = &self.view;

        let key = Self::relationship_key(policy_id, rel);

        let exists = txn
            .has(key.as_bytes())
            .await
            .map_err(|e| Error::Serialization(e.to_string()))?;

        if exists {
            txn.delete(key.as_bytes())
                .await
                .map_err(|e| Error::Serialization(e.to_string()))?;
        }

        Ok(exists)
    }

    async fn has_relationship(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Subject,
    ) -> Result<bool> {
        let txn = &self.view;

        let rel = Relationship::new(resource, object_id, relation, subject.clone());
        let key = Self::relationship_key(policy_id, &rel);

        txn.has(key.as_bytes())
            .await
            .map_err(|e| Error::Serialization(e.to_string()))
    }

    async fn check_permission_direct(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Did,
    ) -> Result<bool> {
        let txn = &self.view;

        let direct = Relationship::with_entity(resource, object_id, relation, subject.clone());
        let direct_key = Self::relationship_key(policy_id, &direct);

        if txn
            .has(direct_key.as_bytes())
            .await
            .map_err(|e| Error::Serialization(e.to_string()))?
        {
            return Ok(true);
        }

        let wildcard = Relationship::new(resource, object_id, relation, Subject::Wildcard);
        let wildcard_key = Self::relationship_key(policy_id, &wildcard);

        if txn
            .has(wildcard_key.as_bytes())
            .await
            .map_err(|e| Error::Serialization(e.to_string()))?
        {
            return Ok(true);
        }

        let prefix = Self::relation_prefix(policy_id, resource, object_id, relation);
        let iter_opts = IterOptions::new()
            .with_validation(true)
            .with_prefix(prefix.into_bytes());

        let mut iter = txn
            .iterator(iter_opts)
            .await
            .map_err(|e| Error::Serialization(e.to_string()))?;

        while let Some(kv) = iter
            .next()
            .await
            .map_err(|e| Error::Serialization(e.to_string()))?
        {
            let rel: Relationship = serde_json::from_slice(&kv.value)?;
            if rel.subject.is_typed_wildcard() {
                return Ok(true);
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
        let txn = &self.view;

        let prefix = Self::relation_prefix(policy_id, resource, object_id, relation);
        let iter_opts = IterOptions::new()
            .with_validation(true)
            .with_prefix(prefix.into_bytes());

        let mut iter = txn
            .iterator(iter_opts)
            .await
            .map_err(|e| Error::Serialization(e.to_string()))?;

        let mut subjects = Vec::new();

        while let Some(kv) = iter
            .next()
            .await
            .map_err(|e| Error::Serialization(e.to_string()))?
        {
            let rel: Relationship = serde_json::from_slice(&kv.value)?;
            subjects.push(rel.subject);
        }

        Ok(subjects)
    }

    async fn get_relation_targets(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
    ) -> Result<Vec<ObjectRef>> {
        let subjects = self
            .get_relation_subjects(policy_id, resource, object_id, relation)
            .await?;

        let targets: Vec<_> = subjects
            .into_iter()
            .filter_map(|s| match s {
                Subject::EntitySet {
                    resource,
                    object_id,
                    ..
                } => Some(ObjectRef::new(resource, object_id)),
                _ => None,
            })
            .collect();

        Ok(targets)
    }

    async fn delete_object_relationships(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<()> {
        let txn = &self.view;

        let prefix = Self::relationship_prefix(policy_id, resource, object_id);
        let iter_opts = IterOptions::new()
            .with_validation(true)
            .with_prefix(prefix.clone().into_bytes());

        let mut keys_to_delete = Vec::new();
        {
            let mut iter = txn
                .iterator(iter_opts)
                .await
                .map_err(|e| Error::Serialization(e.to_string()))?;

            while let Some(kv) = iter
                .next()
                .await
                .map_err(|e| Error::Serialization(e.to_string()))?
            {
                keys_to_delete.push(kv.key);
            }
        }

        for key in keys_to_delete {
            txn.delete(&key)
                .await
                .map_err(|e| Error::Serialization(e.to_string()))?;
        }

        Ok(())
    }
}

pub(super) struct OwnedZanzibarScope(pub datastore::BasicTxn);

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl zanzibar::store::ZanzibarScope for OwnedZanzibarScope {
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    fn store(&self) -> std::sync::Arc<dyn ZanzibarStore> {
        std::sync::Arc::new(TransactionZanzibarStore::new(self.0.acpstore()))
    }
    async fn commit(self: Box<Self>) -> Result<()> {
        self.0
            .commit()
            .await
            .map_err(|error| Error::Serialization(error.to_string()))
    }
}

//! A NAC write whose commit is durable but whose reload fails afterwards must
//! not leave the edge answering from the snapshot taken before the write.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use acp::nac::NodePermission;
use acp::{PersistentZanzibarStore, Policy, Relationship, Subject};
use db::{NacConfig, NacManager};
use identity::Did;
use storage::RegolithStore;
use zanzibar::error::{Error, Result};
use zanzibar::store::{ScopeAttempt, ScopeOutcome, ZanzibarScope, ZanzibarStore};
use zanzibar::types::ObjectRef;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const STRANGER: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";

/// A NAC store whose reads outside a scope fail once a write scope begun
/// after `armed` has committed: the manager's own reload after that commit
/// fails, while reads inside scopes keep working.
struct FailingReloadStore {
    inner: Arc<dyn ZanzibarStore>,
    armed: Arc<AtomicBool>,
    committed: Arc<AtomicBool>,
}

impl FailingReloadStore {
    fn check(&self) -> Result<()> {
        match self.committed.load(Ordering::SeqCst) {
            true => Err(Error::Serialization("reload failed".into())),
            false => Ok(()),
        }
    }
}

/// A write scope that marks the store committed once its own commit is durable.
struct MarkingScope {
    inner: Box<dyn ZanzibarScope>,
    committed: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl ZanzibarScope for MarkingScope {
    fn store(&self) -> Arc<dyn ZanzibarStore> {
        self.inner.store()
    }

    async fn commit(self: Box<Self>) -> Result<()> {
        self.inner.commit().await?;
        self.committed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait::async_trait]
impl ZanzibarStore for FailingReloadStore {
    fn supports_transactions(&self) -> bool {
        self.inner.supports_transactions()
    }

    async fn begin_scope(&self, readonly: bool) -> Result<Option<Box<dyn ZanzibarScope>>> {
        let scope = self.inner.begin_scope(readonly).await?;
        let marking = !readonly && self.armed.load(Ordering::SeqCst);
        Ok(scope.map(|inner| match marking {
            false => inner,
            true => Box::new(MarkingScope {
                inner,
                committed: self.committed.clone(),
            }) as Box<dyn ZanzibarScope>,
        }))
    }

    async fn transact(
        &self,
        max_attempts: u32,
        attempt: ScopeAttempt,
    ) -> Result<Option<ScopeOutcome>> {
        self.inner.transact(max_attempts, attempt).await
    }

    async fn store_policy(&self, policy: &Policy) -> Result<()> {
        self.inner.store_policy(policy).await
    }

    async fn get_policy(&self, policy_id: &str) -> Result<Option<Policy>> {
        self.check()?;
        self.inner.get_policy(policy_id).await
    }

    async fn has_policy(&self, policy_id: &str) -> Result<bool> {
        self.check()?;
        self.inner.has_policy(policy_id).await
    }

    async fn list_policies(&self) -> Result<Vec<Policy>> {
        self.inner.list_policies().await
    }

    async fn next_policy_counter(&self) -> Result<u64> {
        self.inner.next_policy_counter().await
    }

    async fn delete_policy(&self, policy_id: &str) -> Result<bool> {
        self.inner.delete_policy(policy_id).await
    }

    async fn store_relationship(&self, policy_id: &str, rel: &Relationship) -> Result<()> {
        self.inner.store_relationship(policy_id, rel).await
    }

    async fn delete_relationship(&self, policy_id: &str, rel: &Relationship) -> Result<bool> {
        self.inner.delete_relationship(policy_id, rel).await
    }

    async fn has_relationship(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Subject,
    ) -> Result<bool> {
        self.check()?;
        self.inner
            .has_relationship(policy_id, resource, object_id, relation, subject)
            .await
    }

    async fn check_permission_direct(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Did,
    ) -> Result<bool> {
        self.check()?;
        self.inner
            .check_permission_direct(policy_id, resource, object_id, relation, subject)
            .await
    }

    async fn get_relation_subjects(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
    ) -> Result<Vec<Subject>> {
        self.check()?;
        self.inner
            .get_relation_subjects(policy_id, resource, object_id, relation)
            .await
    }

    async fn get_relation_targets(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
    ) -> Result<Vec<ObjectRef>> {
        self.check()?;
        self.inner
            .get_relation_targets(policy_id, resource, object_id, relation)
            .await
    }

    async fn delete_object_relationships(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<()> {
        self.inner
            .delete_object_relationships(policy_id, resource, object_id)
            .await
    }
}

fn did(value: &str) -> Did {
    Did::new(value).unwrap()
}

#[test]
fn an_enable_whose_reload_fails_still_checks_the_edge() {
    workers::block_on(async {
        let store = Arc::new(FailingReloadStore {
            inner: Arc::new(PersistentZanzibarStore::from_store(Arc::new(
                RegolithStore::in_memory().unwrap(),
            ))),
            armed: Arc::new(AtomicBool::new(false)),
            committed: Arc::new(AtomicBool::new(false)),
        });
        let armed = store.armed.clone();
        let nac = NacManager::new(store, NacConfig::new());
        nac.initialize(None).await.unwrap();
        assert!(
            nac.check_permission(&did(STRANGER), NodePermission::DacBypass)
                .await
                .unwrap(),
            "an unconfigured node lets every caller through"
        );

        armed.store(true, Ordering::SeqCst);
        nac.enable(&did(OWNER))
            .await
            .expect_err("the reload after the enable's commit fails");

        let allowed = nac
            .check_permission(&did(STRANGER), NodePermission::DacBypass)
            .await
            .unwrap_or(false);
        assert!(
            !allowed,
            "the enable committed, so a stranger must not pass the edge on the snapshot from before it"
        );
    })
    .unwrap()
}

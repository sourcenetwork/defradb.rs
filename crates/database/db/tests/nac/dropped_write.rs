//! A NAC write whose future is dropped after its commit, before it refreshes
//! the edge snapshot, must not leave the edge answering from the snapshot taken
//! before the write.

use std::future::Future;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::Poll;

use acp::nac::NodePermission;
use acp::{PersistentZanzibarStore, Policy, Relationship, Subject};
use db::{NacConfig, NacManager};
use identity::Did;
use storage::RegolithStore;
use zanzibar::error::Result;
use zanzibar::store::{ScopeAttempt, ScopeOutcome, ZanzibarScope, ZanzibarStore};
use zanzibar::types::ObjectRef;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const STRANGER: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";

/// A NAC store whose reads outside a scope never finish once a write that
/// started after `armed` has committed, until `stalled` is cleared: the
/// manager's reload after that commit is where its caller drops it.
struct StallingReloadStore {
    inner: Arc<dyn ZanzibarStore>,
    armed: Arc<AtomicBool>,
    stalled: Arc<AtomicBool>,
}

impl StallingReloadStore {
    async fn stall(&self) {
        if self.stalled.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
    }
}

#[async_trait::async_trait]
impl ZanzibarStore for StallingReloadStore {
    fn supports_transactions(&self) -> bool {
        self.inner.supports_transactions()
    }

    async fn begin_scope(&self, readonly: bool) -> Result<Option<Box<dyn ZanzibarScope>>> {
        self.inner.begin_scope(readonly).await
    }

    async fn transact(
        &self,
        max_attempts: u32,
        attempt: ScopeAttempt,
    ) -> Result<Option<ScopeOutcome>> {
        let outcome = self.inner.transact(max_attempts, attempt).await?;
        if self.armed.load(Ordering::SeqCst) {
            self.stalled.store(true, Ordering::SeqCst);
        }
        Ok(outcome)
    }

    async fn store_policy(&self, policy: &Policy) -> Result<()> {
        self.inner.store_policy(policy).await
    }

    async fn get_policy(&self, policy_id: &str) -> Result<Option<Policy>> {
        self.stall().await;
        self.inner.get_policy(policy_id).await
    }

    async fn has_policy(&self, policy_id: &str) -> Result<bool> {
        self.stall().await;
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
        self.stall().await;
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
        self.stall().await;
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
        self.stall().await;
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
        self.stall().await;
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

/// Poll `write` until its commit is durable and it waits on the reload after
/// it, then drop it there, as a caller that gives up on it would.
async fn drop_after_commit<W>(write: W, stalled: &AtomicBool)
where
    W: Future,
{
    let mut write = pin!(write);
    std::future::poll_fn(|cx| {
        if write.as_mut().poll(cx).is_ready() {
            panic!("the write finished; the store never stalled its reload");
        }
        match stalled.load(Ordering::SeqCst) {
            true => Poll::Ready(()),
            false => Poll::Pending,
        }
    })
    .await;
}

#[test]
fn a_revoke_dropped_after_its_commit_still_checks_the_edge() {
    workers::block_on(async {
        let store = Arc::new(StallingReloadStore {
            inner: Arc::new(PersistentZanzibarStore::from_store(Arc::new(
                RegolithStore::in_memory().unwrap(),
            ))),
            armed: Arc::new(AtomicBool::new(false)),
            stalled: Arc::new(AtomicBool::new(false)),
        });
        let armed = store.armed.clone();
        let stalled = store.stalled.clone();
        let nac = NacManager::new(store, NacConfig::new());
        nac.initialize(None).await.unwrap();
        nac.enable(&did(OWNER)).await.unwrap();
        nac.add_permission_grant(&did(OWNER), &did(STRANGER), NodePermission::DacBypass)
            .await
            .unwrap();
        assert!(
            nac.check_permission(&did(STRANGER), NodePermission::DacBypass)
                .await
                .unwrap(),
            "the grant is in place before the revoke"
        );

        armed.store(true, Ordering::SeqCst);
        drop_after_commit(
            nac.remove_permission_grant(&did(OWNER), &did(STRANGER), NodePermission::DacBypass),
            &stalled,
        )
        .await;
        armed.store(false, Ordering::SeqCst);
        stalled.store(false, Ordering::SeqCst);

        let allowed = nac
            .check_permission(&did(STRANGER), NodePermission::DacBypass)
            .await
            .unwrap();
        assert!(
            !allowed,
            "the revoke committed, so the stranger must not pass the edge on the snapshot from before it"
        );
    })
    .unwrap()
}

//! Disabling and re-enabling NAC decide in the write that changes the status:
//! a revocation or another status change that commits while the write is open
//! fails its commit, and the decision runs again on what committed.
//!
//! Each test races a second node handle over the same NAC store against the
//! first: the race runs once, just as the first handle's write stores or
//! deletes the `_disabled` relationship, and commits before that write does.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use acp::nac::{NacStatus, NodePermission};
use acp::{PersistentZanzibarStore, Policy, Relationship, Subject};
use db::error::Error;
use db::{NacConfig, NacManager, NodeAcpError, DB};
use defra_core::RequestContext;
use identity::Did;
use storage::RegolithStore;
use zanzibar::error::Result;
use zanzibar::store::{ScopeAttempt, ScopeOutcome, ZanzibarScope, ZanzibarStore};
use zanzibar::types::ObjectRef;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const WRITER: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";

/// What commits in the middle of the first handle's write.
type Race = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// A NAC store whose write scopes run `race` once, at their first
/// relationship write.
struct RacingStore {
    inner: Arc<dyn ZanzibarStore>,
    race: Race,
    armed: Arc<AtomicBool>,
}

impl RacingStore {
    fn over(inner: Arc<dyn ZanzibarStore>, race: Race, armed: Arc<AtomicBool>) -> Self {
        Self { inner, race, armed }
    }

    async fn race_once(&self) {
        if self.armed.swap(false, Ordering::SeqCst) {
            (self.race)().await;
        }
    }
}

/// A write scope whose store is a [`RacingStore`] over the scope's own.
struct RacingScope {
    inner: Box<dyn ZanzibarScope>,
    race: Race,
    armed: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl ZanzibarScope for RacingScope {
    fn store(&self) -> Arc<dyn ZanzibarStore> {
        Arc::new(RacingStore::over(
            self.inner.store(),
            self.race.clone(),
            self.armed.clone(),
        ))
    }

    async fn commit(self: Box<Self>) -> Result<()> {
        self.inner.commit().await
    }
}

#[async_trait::async_trait]
impl ZanzibarStore for RacingStore {
    fn supports_transactions(&self) -> bool {
        self.inner.supports_transactions()
    }

    async fn begin_scope(&self, readonly: bool) -> Result<Option<Box<dyn ZanzibarScope>>> {
        let scope = self.inner.begin_scope(readonly).await?;
        Ok(scope.map(|inner| match readonly {
            true => inner,
            false => Box::new(RacingScope {
                inner,
                race: self.race.clone(),
                armed: self.armed.clone(),
            }) as Box<dyn ZanzibarScope>,
        }))
    }

    async fn transact(
        &self,
        max_attempts: u32,
        mut attempt: ScopeAttempt,
    ) -> Result<Option<ScopeOutcome>> {
        let (race, armed) = (self.race.clone(), self.armed.clone());
        let racing: ScopeAttempt = Box::new(move |store| {
            attempt(Arc::new(RacingStore::over(
                store,
                race.clone(),
                armed.clone(),
            )))
        });
        self.inner.transact(max_attempts, racing).await
    }

    async fn store_policy(&self, policy: &Policy) -> Result<()> {
        self.inner.store_policy(policy).await
    }

    async fn get_policy(&self, policy_id: &str) -> Result<Option<Policy>> {
        self.inner.get_policy(policy_id).await
    }

    async fn has_policy(&self, policy_id: &str) -> Result<bool> {
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
        self.race_once().await;
        self.inner.store_relationship(policy_id, rel).await
    }

    async fn delete_relationship(&self, policy_id: &str, rel: &Relationship) -> Result<bool> {
        self.race_once().await;
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

fn acting(value: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(value.to_string()))
}

/// A node handle over `store`, with NAC owned by [`OWNER`].
async fn node_over<S>(store: Arc<S>) -> (Arc<DB<RegolithStore>>, Arc<NacManager<S>>)
where
    S: ZanzibarStore + 'static,
{
    node_with(store, NacConfig::new().with_enabled()).await
}

/// [`node_over`] with `config`.
async fn node_with<S>(
    store: Arc<S>,
    config: NacConfig,
) -> (Arc<DB<RegolithStore>>, Arc<NacManager<S>>)
where
    S: ZanzibarStore + 'static,
{
    let nac = Arc::new(NacManager::new(store, config));
    nac.initialize(Some(&did(OWNER))).await.unwrap();
    let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    db.set_nac_manager(nac.clone());
    (Arc::new(db), nac)
}

/// The NAC store both handles share.
type Shared = Arc<PersistentZanzibarStore<RegolithStore>>;

/// The first handle, whose write scopes run `race` once, and the store it
/// shares with the second handle, which `prepare` sets up first and `race`
/// acts through.
async fn racing_nodes<F>(
    prepare: impl AsyncFnOnce(&DB<RegolithStore>, &NacManager<PersistentZanzibarStore<RegolithStore>>),
    race: F,
) -> (Arc<DB<RegolithStore>>, Shared)
where
    F: Fn(Arc<DB<RegolithStore>>) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync
        + 'static,
{
    racing_nodes_with(NacConfig::new().with_enabled(), prepare, race).await
}

/// [`racing_nodes`] whose first handle runs on `config`.
async fn racing_nodes_with<F>(
    config: NacConfig,
    prepare: impl AsyncFnOnce(&DB<RegolithStore>, &NacManager<PersistentZanzibarStore<RegolithStore>>),
    race: F,
) -> (Arc<DB<RegolithStore>>, Shared)
where
    F: Fn(Arc<DB<RegolithStore>>) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync
        + 'static,
{
    let shared = Arc::new(PersistentZanzibarStore::from_store(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));
    let (other_db, other) = node_over(shared.clone()).await;
    prepare(&*other_db, &*other).await;
    let race: Race = Arc::new(move || race(other_db.clone()));
    let racing = RacingStore::over(shared.clone(), race, Arc::new(AtomicBool::new(true)));
    let (db, _) = node_with(Arc::new(racing), config).await;
    (db, shared)
}

/// The status `shared` holds now, read afresh.
async fn stored_status(shared: Shared) -> NacStatus {
    let reader = NacManager::new(shared, NacConfig::new());
    reader.initialize(None).await.unwrap();
    reader.status().await
}

#[test]
fn a_revocation_that_commits_first_denies_the_disable() {
    workers::block_on(async {
        let (db, shared) = racing_nodes(
            async |_, other| {
                other
                    .add_permission_grant(&did(OWNER), &did(WRITER), NodePermission::NacDisable)
                    .await
                    .unwrap();
            },
            |other_db| {
                Box::pin(async move {
                    let nac = other_db.nac_manager().unwrap();
                    nac.remove_permission_grant(
                        &did(OWNER),
                        &did(WRITER),
                        NodePermission::NacDisable,
                    )
                    .await
                    .unwrap();
                })
            },
        )
        .await;

        let error = db.disable_nac_as(&acting(WRITER)).await.unwrap_err();
        assert!(
            matches!(error, NodeAcpError::Db(Error::NotAuthorized { .. })),
            "a revoked caller must be denied, got: {error:?}"
        );
        assert_eq!(stored_status(shared.clone()).await, NacStatus::Enabled);
    })
    .unwrap()
}

#[test]
fn of_two_disables_one_wins_and_the_other_is_already_disabled() {
    workers::block_on(async {
        let (db, shared) = racing_nodes(
            async |_, _| {},
            |other_db| {
                Box::pin(async move {
                    other_db.disable_nac_as(&acting(OWNER)).await.unwrap();
                })
            },
        )
        .await;

        let error = db.disable_nac_as(&acting(OWNER)).await.unwrap_err();
        assert!(
            matches!(error, NodeAcpError::AlreadyDisabled),
            "the losing disable must hear Go's error, got: {error:?}"
        );
        assert_eq!(error.to_string(), "node acp is already disabled");
        assert_eq!(
            stored_status(shared.clone()).await,
            NacStatus::DisabledTemporarily
        );
    })
    .unwrap()
}

#[test]
fn of_two_re_enables_one_wins_and_the_other_is_already_enabled() {
    workers::block_on(async {
        let (db, shared) = racing_nodes(
            async |other_db, _| {
                other_db.disable_nac_as(&acting(OWNER)).await.unwrap();
            },
            |other_db| {
                Box::pin(async move {
                    other_db.re_enable_nac_as(&acting(OWNER)).await.unwrap();
                })
            },
        )
        .await;
        assert_eq!(
            stored_status(shared.clone()).await,
            NacStatus::DisabledTemporarily
        );

        let error = db.re_enable_nac_as(&acting(OWNER)).await.unwrap_err();
        assert!(
            matches!(error, NodeAcpError::AlreadyEnabled),
            "the losing re-enable must hear Go's error, got: {error:?}"
        );
        assert_eq!(error.to_string(), "node acp is already enabled");
        assert_eq!(stored_status(shared.clone()).await, NacStatus::Enabled);
    })
    .unwrap()
}

#[test]
fn a_disable_that_commits_first_refuses_the_relationship_with_gos_error() {
    workers::block_on(async {
        let (db, _) = racing_nodes(
            async |_, _| {},
            |other_db| {
                Box::pin(async move {
                    other_db.disable_nac_as(&acting(OWNER)).await.unwrap();
                })
            },
        )
        .await;

        let error = db
            .add_nac_actor_relationship_as(&acting(OWNER), "admin", WRITER)
            .await
            .unwrap_err();
        assert!(
            matches!(error, NodeAcpError::AcpNotAvailable),
            "a relationship change after a disable must hear Go's error, got: {error:?}"
        );
        assert_eq!(
            error.to_string(),
            "operation requires ACP, but ACP not available"
        );
    })
    .unwrap()
}

#[test]
fn a_write_that_loses_every_try_is_a_conflict() {
    workers::block_on(async {
        let (db, shared) = racing_nodes_with(
            NacConfig::new().with_enabled().with_max_txn_retries(0),
            async |_, _| {},
            |other_db| {
                Box::pin(async move {
                    other_db.disable_nac_as(&acting(OWNER)).await.unwrap();
                })
            },
        )
        .await;

        let error = db.disable_nac_as(&acting(OWNER)).await.unwrap_err();
        assert!(
            error.is_conflict(),
            "a write with no retries left must report the conflict, got: {error:?}"
        );
        assert_eq!(
            stored_status(shared.clone()).await,
            NacStatus::DisabledTemporarily
        );
    })
    .unwrap()
}

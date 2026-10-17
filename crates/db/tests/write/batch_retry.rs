use crate::common::schema::test_collection;
use acp::{DocumentACP, Identity};
use async_trait::async_trait;
use db::{DbCollectionProvider, LensedAutoCommitFetcher, DB};
use events::{Bus, ChannelBus, EventName};
use query::{DocFetcher, DocMutator, QueryExecutor, QueryRequest};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::{future::Future, pin::Pin};
use storage::corekv::{RetryAttempt, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture};
use storage::{RegolithStore, Result, Store, Txn};

const ACP_PERMISSION_KEY: &[u8] = b"batch-retry-acp-permission";

/// A remote-provider stand-in: it delegates storage to local ACP memory but
/// deliberately disables transaction binding so registrations only happen in
/// the post-commit ordered hook.
struct RemoteRegistrationAcp {
    inner: Arc<acp::LocalDocumentACP>,
    registrations: AtomicUsize,
}

#[async_trait]
impl DocumentACP for RemoteRegistrationAcp {
    fn supports_transactions(&self) -> bool {
        false
    }

    async fn register_doc_object(
        &self,
        identity: &identity::Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<()> {
        self.registrations.fetch_add(1, Ordering::SeqCst);
        self.inner
            .register_doc_object(identity, policy_id, resource_name, doc_id)
            .await
    }

    async fn is_doc_registered(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<bool> {
        self.inner
            .is_doc_registered(policy_id, resource_name, doc_id)
            .await
    }

    async fn check_doc_access(
        &self,
        identity: &Identity,
        permission: acp::DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<bool> {
        self.inner
            .check_doc_access(identity, permission, policy_id, resource_name, doc_id)
            .await
    }

    async fn add_actor_relationship(
        &self,
        requestor: &identity::Did,
        target: &identity::Did,
        policy_id: &str,
        collection_id: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> acp::Result<bool> {
        self.inner
            .add_actor_relationship(
                requestor,
                target,
                policy_id,
                collection_id,
                doc_id,
                relation,
                managing_relations,
            )
            .await
    }

    async fn delete_actor_relationship(
        &self,
        requestor: &identity::Did,
        target: &identity::Did,
        policy_id: &str,
        collection_id: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> acp::Result<bool> {
        self.inner
            .delete_actor_relationship(
                requestor,
                target,
                policy_id,
                collection_id,
                doc_id,
                relation,
                managing_relations,
            )
            .await
    }

    async fn unregister_doc_object(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> acp::Result<()> {
        self.inner
            .unregister_doc_object(policy_id, resource_name, doc_id)
            .await
    }
}

struct RevokeAfterAttemptRead {
    store: Arc<RegolithStore>,
    view: Option<datastore::NamespaceView>,
    revoked: Arc<AtomicBool>,
    permission_checks: Arc<AtomicUsize>,
    attempt_reads: Arc<AtomicUsize>,
}

impl RevokeAfterAttemptRead {
    async fn read_permission(&self) -> acp::Result<Option<bytes::Bytes>> {
        if let Some(view) = &self.view {
            self.attempt_reads.fetch_add(1, Ordering::SeqCst);
            return view
                .get(ACP_PERMISSION_KEY)
                .await
                .map_err(|error| acp::Error::Storage(error.to_string()));
        }
        let txn = datastore::BasicTxn::new(self.store.as_ref(), 990_002, true)
            .await
            .map_err(|error| acp::Error::Storage(error.to_string()))?;
        txn.acpstore()
            .get(ACP_PERMISSION_KEY)
            .await
            .map_err(|error| acp::Error::Storage(error.to_string()))
    }

    async fn revoke_outside_attempt(&self) -> acp::Result<()> {
        let txn = datastore::BasicTxn::new(self.store.as_ref(), 990_001, false)
            .await
            .map_err(|error| acp::Error::Storage(error.to_string()))?;
        txn.acpstore()
            .set(ACP_PERMISSION_KEY, b"deny")
            .await
            .map_err(|error| acp::Error::Storage(error.to_string()))?;
        txn.commit()
            .await
            .map_err(|error| acp::Error::Storage(error.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentACP for RevokeAfterAttemptRead {
    fn supports_transactions(&self) -> bool {
        true
    }

    fn bind_transaction(&self, view: datastore::NamespaceView) -> Option<Arc<dyn DocumentACP>> {
        Some(Arc::new(Self {
            store: self.store.clone(),
            view: Some(view),
            revoked: self.revoked.clone(),
            permission_checks: self.permission_checks.clone(),
            attempt_reads: self.attempt_reads.clone(),
        }))
    }

    fn unregistered_documents_are_public(&self) -> bool {
        false
    }

    async fn register_doc_object(
        &self,
        _identity: &identity::Did,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<()> {
        Ok(())
    }

    async fn is_doc_registered(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<bool> {
        let value = self.read_permission().await?;
        Ok(value.is_some())
    }

    async fn get_doc_owner(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<Option<identity::Did>> {
        Ok(None)
    }

    async fn check_doc_access(
        &self,
        _identity: &Identity,
        permission: acp::DocumentPermission,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<bool> {
        self.permission_checks.fetch_add(1, Ordering::SeqCst);
        let value = self.read_permission().await?;
        // Keep read access so a revoked update is an explicit denial,
        // rather than the documented empty result for an invisible document.
        let allowed = permission == acp::DocumentPermission::Read
            || value.is_some_and(|value| value.as_ref() == b"allow");
        if allowed && self.view.is_some() && !self.revoked.swap(true, Ordering::SeqCst) {
            self.revoke_outside_attempt().await?;
        }
        Ok(allowed)
    }

    async fn add_actor_relationship(
        &self,
        _requestor: &identity::Did,
        _target: &identity::Did,
        _policy_id: &str,
        _collection_id: &str,
        _doc_id: &str,
        _relation: &str,
        _managing_relations: &[String],
    ) -> acp::Result<bool> {
        Ok(false)
    }

    async fn delete_actor_relationship(
        &self,
        _requestor: &identity::Did,
        _target: &identity::Did,
        _policy_id: &str,
        _collection_id: &str,
        _doc_id: &str,
        _relation: &str,
        _managing_relations: &[String],
    ) -> acp::Result<bool> {
        Ok(false)
    }

    async fn unregister_doc_object(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        _doc_id: &str,
    ) -> acp::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
struct ConflictStore {
    inner: RegolithStore,
    attempts: Arc<AtomicUsize>,
    raced: Arc<AtomicBool>,
    attempt_ids: Arc<Mutex<Vec<Vec<String>>>>,
    race_operation: Arc<Mutex<Option<RaceOperation>>>,
}

type RaceOperation =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

impl storage::corekv::private::Sealed for ConflictStore {}

#[async_trait]
impl Store for ConflictStore {
    async fn allocate(&self, key: &[u8], count: u64) -> Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }
    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        self.inner.new_txn(readonly).await
    }
    async fn transact_retry(
        &self,
        options: storage::corekv::RetryOptions,
        attempt: RetryAttempt,
    ) -> std::result::Result<storage::corekv::RetryAttemptValue, RetryAttemptError> {
        struct InjectConflict {
            store: RegolithStore,
            attempts: Arc<AtomicUsize>,
            raced: Arc<AtomicBool>,
            attempt_ids: Arc<Mutex<Vec<Vec<String>>>>,
            race_operation: Arc<Mutex<Option<RaceOperation>>>,
            inner: RetryAttempt,
        }
        impl RetryAttemptFn for InjectConflict {
            fn run<'a>(
                &'a mut self,
                txn: &'a mut dyn storage::corekv::RetryTxn,
            ) -> RetryAttemptFuture<'a> {
                Box::pin(async move {
                    if !self.raced.swap(true, Ordering::SeqCst) {
                        let race_operation = self.race_operation.lock().unwrap().clone();
                        if let Some(race_operation) = race_operation {
                            race_operation().await?;
                        } else {
                            txn.get(b"batch-conflict").await?;
                            txn.set(b"batch-conflict", b"loser").await?;
                            let mut winner = self.store.new_txn(false).await?;
                            winner.set(b"batch-conflict", b"winner").await?;
                            winner.commit().await?;
                        }
                    }
                    let result = self.inner.run(txn).await?;
                    self.attempts.fetch_add(1, Ordering::SeqCst);
                    if let Some(value) = result.downcast_ref::<serde_json::Value>() {
                        let mut ids = Vec::new();
                        collect_doc_ids(value, &mut ids);
                        self.attempt_ids.lock().unwrap().push(ids);
                    }
                    Ok(result)
                })
            }
        }
        self.inner
            .transact_retry(
                options,
                Box::new(InjectConflict {
                    store: self.inner.clone(),
                    attempts: self.attempts.clone(),
                    raced: self.raced.clone(),
                    attempt_ids: self.attempt_ids.clone(),
                    race_operation: self.race_operation.clone(),
                    inner: attempt,
                }),
            )
            .await
    }
    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}

fn collect_doc_ids(value: &serde_json::Value, ids: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(id) = map.get("_docID").and_then(serde_json::Value::as_str) {
                ids.push(id.to_owned());
            }
            for child in map.values() {
                collect_doc_ids(child, ids);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_doc_ids(child, ids);
            }
        }
        _ => {}
    }
}

#[test]
fn implicit_batch_replays_atomically_inside_engine_attempt() {
    workers::block_on(async {
        let inner = RegolithStore::in_memory().unwrap();
        let store = ConflictStore {
            inner: inner.clone(),
            attempts: Arc::new(AtomicUsize::new(0)),
            raced: Arc::new(AtomicBool::new(false)),
            attempt_ids: Arc::new(Mutex::new(Vec::new())),
            race_operation: Arc::new(Mutex::new(None)),
        };
        let bus: Arc<dyn Bus> = Arc::new(ChannelBus::new());
        let mut db = DB::new(store.clone()).unwrap();
        db.set_event_bus(bus.clone());
        let db = Arc::new(db);
        db.create_collection(test_collection()).await.unwrap();

        let runner = query::QueryRunner::with_provider(
            LensedAutoCommitFetcher::new(db.clone()),
            DbCollectionProvider::new_arc(db.clone()),
        )
        .with_mutator(Arc::new(db::write::autocommit::AutoCommitMutator::new(
            db.clone(),
        )));
        let request = QueryRequest::new(
            r#"mutation {
            first: add_TestDoc(input: {x: 1}) { _docID }
            second: add_TestDoc(input: {x: 2}) { _docID }
        }"#,
        );
        let prepared = runner.prepare_request(&request).await.unwrap();
        assert!(
            prepared.is_some(),
            "a top-level batch must be request-prepared"
        );
        let mut events = bus.subscribe(&[EventName::Update]);
        let response = runner.execute_prepared(request, prepared).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(store.attempts.load(Ordering::SeqCst), 2);
        let mut returned = {
            let attempt_ids = store.attempt_ids.lock().unwrap();
            assert_eq!(attempt_ids.len(), 2);
            assert_eq!(attempt_ids[0], attempt_ids[1]);
            attempt_ids[1].clone()
        };
        assert_eq!(returned.len(), 2);
        let mut doc_events = Vec::new();
        for _ in 0..2 {
            let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
                .await
                .expect("winner event arrives")
                .expect("subscription remains open");
            doc_events.push(event.as_update().unwrap().doc_id.clone());
        }
        doc_events.sort();
        returned.sort();
        assert_eq!(doc_events, returned);
        assert!(
            events.try_recv().is_err(),
            "losing attempt must publish no events"
        );

        let docs = LensedAutoCommitFetcher::new(db)
            .get_all("TestDoc")
            .await
            .unwrap();
        assert_eq!(docs.len(), 2, "the whole batch commits atomically");
    })
    .unwrap()
}

#[test]
fn implicit_single_update_is_prepared_for_engine_owned_retry() {
    workers::block_on(async {
        let inner = RegolithStore::in_memory().unwrap();
        let store = ConflictStore {
            inner: inner.clone(),
            attempts: Arc::new(AtomicUsize::new(0)),
            raced: Arc::new(AtomicBool::new(true)),
            attempt_ids: Arc::new(Mutex::new(Vec::new())),
            race_operation: Arc::new(Mutex::new(None)),
        };
        let store_db = Arc::new(DB::new(store.clone()).unwrap());
        store_db.create_collection(test_collection()).await.unwrap();
        let created = db::write::autocommit::AutoCommitMutator::new(store_db.clone())
            .create(
                "TestDoc",
                document::Document::from_json_str(r#"{"x": 1}"#).unwrap(),
            )
            .await
            .unwrap();
        let race_db = store_db.clone();
        let race_doc_id = created.doc_id.clone();
        *store.race_operation.lock().unwrap() = Some(Arc::new(move || {
            let db = race_db.clone();
            let doc_id = race_doc_id.clone();
            Box::pin(async move {
                let mutator = db::write::autocommit::AutoCommitMutator::new(db);
                let mut doc = mutator
                    .get_for_update("TestDoc", &doc_id)
                    .await
                    .map_err(|error| storage::Error::Other(error.to_string()))?
                    .ok_or_else(|| storage::Error::Other("test document disappeared".into()))?;
                doc.set("x", 3_i64);
                mutator
                    .update(
                        "TestDoc",
                        doc,
                        rapidhash::RapidHashSet::from_iter(["x".to_string()]),
                    )
                    .await
                    .map_err(|error| storage::Error::Other(error.to_string()))?;
                Ok(())
            })
        }));

        let runner = query::QueryRunner::with_provider(
            LensedAutoCommitFetcher::new(store_db.clone()),
            DbCollectionProvider::new_arc(store_db.clone()),
        )
        .with_mutator(Arc::new(db::write::autocommit::AutoCommitMutator::new(
            store_db.clone(),
        )));
        let mut request = QueryRequest::new(
            r#"mutation { update_TestDoc(filter: {x: {_eq: 1}}, input: {x: 2}) { _docID x } }"#,
        );
        request.identity = Some(identity::Did::new("did:key:z6Mkalice").unwrap());
        let prepared = runner.prepare_request(&request).await.unwrap();
        assert!(
            prepared
                .as_ref()
                .is_some_and(|prepared| prepared.engine_managed_retry),
            "single implicit mutations must use the engine-owned retry boundary"
        );

        store.attempts.store(0, Ordering::SeqCst);
        store.raced.store(false, Ordering::SeqCst);
        let response = runner.execute_prepared(request, prepared).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        // Counts include the injected concurrent update as well as the two
        // complete request-attempt bodies.
        assert_eq!(store.attempts.load(Ordering::SeqCst), 3);
        {
            let attempt_ids = store.attempt_ids.lock().unwrap();
            assert_eq!(attempt_ids.len(), 2);
            assert_eq!(attempt_ids[0], vec![created.doc_id.to_string()]);
            assert!(
                attempt_ids[1].is_empty(),
                "selection must rerun on fresh snapshot"
            );
        }
        assert!(response.data.unwrap()["update_TestDoc"]
            .as_array()
            .unwrap()
            .is_empty());
        let final_doc = db::write::autocommit::AutoCommitMutator::new(store_db)
            .get_for_update("TestDoc", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(final_doc.get("x"), Some(&document::NormalValue::Int(3)));
    })
    .unwrap()
}

#[test]
fn direct_mutation_api_replays_atomically_inside_engine_attempt() {
    workers::block_on(async {
        let inner = RegolithStore::in_memory().unwrap();
        let store = ConflictStore {
            inner: inner.clone(),
            attempts: Arc::new(AtomicUsize::new(0)),
            raced: Arc::new(AtomicBool::new(false)),
            attempt_ids: Arc::new(Mutex::new(Vec::new())),
            race_operation: Arc::new(Mutex::new(None)),
        };
        let bus: Arc<dyn Bus> = Arc::new(ChannelBus::new());
        let mut db = DB::new(store.clone()).unwrap();
        db.set_event_bus(bus.clone());
        let db = Arc::new(db);
        db.create_collection(test_collection()).await.unwrap();

        let runner = query::QueryRunner::with_provider(
            LensedAutoCommitFetcher::new(db.clone()),
            DbCollectionProvider::new_arc(db.clone()),
        )
        .with_mutator(Arc::new(db::write::autocommit::AutoCommitMutator::new(
            db.clone(),
        )));
        let mut events = bus.subscribe(&[EventName::Update]);
        let response = runner
            .execute_mutation_with_identity_and_vars(
                r#"mutation {
                first: add_TestDoc(input: {x: 1}) { _docID }
                second: add_TestDoc(input: {x: 2}) { _docID }
            }"#,
                None,
                None,
            )
            .await
            .unwrap();
        assert!(response.is_object());
        assert_eq!(store.attempts.load(Ordering::SeqCst), 2);
        let mut returned = {
            let attempt_ids = store.attempt_ids.lock().unwrap();
            assert_eq!(attempt_ids.len(), 2);
            assert_eq!(attempt_ids[0], attempt_ids[1]);
            attempt_ids[1].clone()
        };
        assert_eq!(returned.len(), 2);
        let mut doc_events = Vec::new();
        for _ in 0..2 {
            let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
                .await
                .expect("winner event arrives")
                .expect("subscription remains open");
            doc_events.push(event.as_update().unwrap().doc_id.clone());
        }
        doc_events.sort();
        returned.sort();
        assert_eq!(doc_events, returned);
        assert!(
            events.try_recv().is_err(),
            "losing attempt must publish no events"
        );

        let docs = LensedAutoCommitFetcher::new(db)
            .get_all("TestDoc")
            .await
            .unwrap();
        assert_eq!(docs.len(), 2, "the direct API batch commits atomically");
    })
    .unwrap()
}

#[test]
fn single_create_replays_after_engine_conflict_with_consistent_identity_and_cid() {
    workers::block_on(async {
        let inner = RegolithStore::in_memory().unwrap();
        let store = ConflictStore {
            inner: inner.clone(),
            attempts: Arc::new(AtomicUsize::new(0)),
            raced: Arc::new(AtomicBool::new(false)),
            attempt_ids: Arc::new(Mutex::new(Vec::new())),
            race_operation: Arc::new(Mutex::new(None)),
        };
        let db = Arc::new(DB::new(store.clone()).unwrap());
        db.create_collection(test_collection()).await.unwrap();

        let result = db::write::autocommit::AutoCommitMutator::new(db.clone())
            .create(
                "TestDoc",
                document::Document::from_json_str(r#"{"x":7}"#).unwrap(),
            )
            .await
            .unwrap();

        assert!(
            store.raced.load(Ordering::SeqCst),
            "the first commit must conflict"
        );
        assert_eq!(result.document.id(), Some(&result.doc_id));
        let block = result.commit_block.expect("created document block");
        let cid = result.commit_cid.expect("created document CID");
        assert_eq!(
            cid,
            defra_core::block::generate_cid_from_bytes(&block).unwrap(),
            "returned CID must identify the exact returned block bytes"
        );
        assert!(
            !db::AutoCommitFetcher::new(db)
                .get_by_ids("TestDoc", &[result.doc_id.to_string()])
                .await
                .unwrap()
                .docs()
                .is_empty(),
            "the returned ID must resolve to the committed document"
        );
    })
    .unwrap()
}

#[test]
fn transactional_acp_revoke_conflicts_and_denies_batch_update() {
    workers::block_on(async {
        let inner = RegolithStore::in_memory().unwrap();
        let store = ConflictStore {
            inner: inner.clone(),
            attempts: Arc::new(AtomicUsize::new(0)),
            raced: Arc::new(AtomicBool::new(true)),
            attempt_ids: Arc::new(Mutex::new(Vec::new())),
            race_operation: Arc::new(Mutex::new(None)),
        };
        let db = Arc::new(DB::new(store.clone()).unwrap());
        db.create_collection(
            test_collection().with_policy(schema::PolicyDescription::new("policy", "test-docs")),
        )
        .await
        .unwrap();

        let original = db::write::autocommit::AutoCommitMutator::new(db.clone())
            .create(
                "TestDoc",
                document::Document::from_json_str(r#"{"x":1}"#).unwrap(),
            )
            .await
            .unwrap();
        let seed = datastore::BasicTxn::new(&inner, 990_000, false)
            .await
            .unwrap();
        seed.acpstore()
            .set(ACP_PERMISSION_KEY, b"allow")
            .await
            .unwrap();
        seed.commit().await.unwrap();

        let permission_checks = Arc::new(AtomicUsize::new(0));
        let attempt_reads = Arc::new(AtomicUsize::new(0));
        let acp: Arc<dyn DocumentACP> = Arc::new(RevokeAfterAttemptRead {
            store: Arc::new(inner.clone()),
            view: None,
            revoked: Arc::new(AtomicBool::new(false)),
            permission_checks: permission_checks.clone(),
            attempt_reads: attempt_reads.clone(),
        });
        let runner = query::QueryRunner::with_provider(
            LensedAutoCommitFetcher::new(db.clone()),
            DbCollectionProvider::new_arc(db.clone()),
        )
        .with_mutator(Arc::new(db::write::autocommit::AutoCommitMutator::new(
            db.clone(),
        )))
        .with_acp(acp);
        let mut request = QueryRequest::new(format!(
            r#"mutation {{ update_TestDoc(docID: "{}", input: {{x: 2}}) {{ _docID x }} }}"#,
            original.doc_id
        ));
        request.identity = Some(identity::Did::new("did:key:batch-reader").unwrap());
        let prepared = runner.prepare_request(&request).await.unwrap();
        // Count only the request attempts, excluding setup and preparation.
        store.attempts.store(0, Ordering::SeqCst);
        permission_checks.store(0, Ordering::SeqCst);
        let response = runner.execute_prepared(request, prepared).await;

        assert!(
            response.has_errors(),
            "revoked authorization must deny the retry: {:?}, checks={}, attempts={}",
            response.errors,
            permission_checks.load(Ordering::SeqCst),
            store.attempts.load(Ordering::SeqCst)
        );
        assert_eq!(store.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(permission_checks.load(Ordering::SeqCst), 4);
        assert!(attempt_reads.load(Ordering::SeqCst) >= 2);
        let docs = LensedAutoCommitFetcher::new(db)
            .get_all("TestDoc")
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].get("x"), Some(&document::NormalValue::Int(1)));
    })
    .unwrap()
}

#[test]
fn remote_acp_batch_replays_with_attempt_local_registration_projection() {
    workers::block_on(async {
        let inner = RegolithStore::in_memory().unwrap();
        let store = ConflictStore {
            inner: inner.clone(),
            attempts: Arc::new(AtomicUsize::new(0)),
            raced: Arc::new(AtomicBool::new(false)),
            attempt_ids: Arc::new(Mutex::new(Vec::new())),
            race_operation: Arc::new(Mutex::new(None)),
        };
        let bus: Arc<dyn Bus> = Arc::new(ChannelBus::new());
        let mut db = DB::new(store.clone()).unwrap();
        db.set_event_bus(bus);
        let owner =
            identity::Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
        db.create_collection(
            test_collection().with_policy(schema::PolicyDescription::new("policy1", "test-docs")),
        )
        .await
        .unwrap();
        let acp = Arc::new(RemoteRegistrationAcp {
            inner: Arc::new(acp::LocalDocumentACP::new(Arc::new(
                acp::MemoryAcpStore::new(),
            ))),
            registrations: AtomicUsize::new(0),
        });
        // A non-transactional provider is external: reads and writes go through
        // the gate of owed registrations, as every node wires it.
        db.set_document_acp(Arc::new(acp::ExternalDocumentAcp::new(
            acp.clone(),
            db.store().clone(),
        )));
        let db = Arc::new(db);

        let runner = query::QueryRunner::with_provider(
            LensedAutoCommitFetcher::new(db.clone()),
            DbCollectionProvider::new_arc(db.clone()),
        )
        .with_mutator(Arc::new(db::write::autocommit::AutoCommitMutator::new(
            db.clone(),
        )));
        let request = QueryRequest::new(
            r#"mutation {
            first: add_TestDoc(input: {x: 1}) { _docID }
            second: add_TestDoc(input: {x: 2}) { _docID }
        }"#,
        )
        .with_identity(Some(owner.clone()));
        let response =
            defra_core::current_identity::with_scoped_identity(Some(owner.to_string()), async {
                defra_core::signing::scope_request_bearer_token(
                    Some(owner.to_string()),
                    Some("batch-request-token".to_string()),
                    async {
                        let prepared = runner.prepare_request(&request).await.unwrap();
                        assert!(
                            prepared.is_some(),
                            "remote ACP batches are request-prepared"
                        );
                        defra_core::signing::scope_broadcast_creator_did(
                            Some(owner.to_string()),
                            runner.execute_prepared(request, prepared),
                        )
                        .await
                    },
                )
                .await
            })
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(store.attempts.load(Ordering::SeqCst), 2);
        let winner_ids = {
            let attempt_ids = store.attempt_ids.lock().unwrap();
            assert_eq!(attempt_ids.len(), 2);
            assert_eq!(attempt_ids[0], attempt_ids[1]);
            attempt_ids[1].clone()
        };

        for id in winner_ids {
            assert!(
                acp.is_doc_registered("policy1", "test-docs", &id)
                    .await
                    .unwrap(),
                "only committed batch documents should be registered"
            );
        }
        assert_eq!(
            acp.registrations.load(Ordering::SeqCst),
            2,
            "only the winning commit's two registration hooks should run"
        );
    })
    .unwrap()
}

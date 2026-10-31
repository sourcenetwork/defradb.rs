//! Purge by DocID (`go.shinzo.purge`): what the purge leaves in the block
//! store and the ownership records, which the scenario suite cannot see.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use cid::Cid;
use db::{AutoCommitMutator, DB};
use document::{DocID, Document, NormalValue};
use query::txn::TransactionRegistry;
use query::DocMutator;
use rapidhash::RapidHashSet;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::corekv::{
    private::Sealed, Result as KvResult, RetryAttempt, RetryAttemptError, RetryAttemptFn,
    RetryAttemptFuture, RetryAttemptValue, RetryOptions, RetryTxn, Store, Txn,
};
use storage::keys::doc_id_index::BlockCIDToDocIDKey;
use storage::namespace::Namespace;
use storage::RegolithStore;

fn users_schema() -> CollectionVersion {
    CollectionVersion::new(
        "Users",
        "users-v1",
        "users-collection",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
            FieldDescription::new("3", "age", FieldKind::int(), schema::CType::LwwRegister),
        ],
    )
}

async fn create<S>(db: &Arc<DB<S>>, name: &str, age: i64) -> DocID
where
    S: Store + 'static,
{
    let mut doc = Document::new(users_schema());
    doc.set("name", NormalValue::String(name.to_owned()))
        .unwrap();
    doc.set("age", NormalValue::Int(age)).unwrap();
    AutoCommitMutator::new(db.clone())
        .create("Users", doc)
        .await
        .unwrap()
        .doc_id
}

async fn set_age<S>(db: &Arc<DB<S>>, doc_id: &DocID, age: i64)
where
    S: Store + 'static,
{
    let mut doc = Document::new(users_schema());
    doc.set_id(doc_id.clone());
    doc.set("age", NormalValue::Int(age)).unwrap();
    AutoCommitMutator::new(db.clone())
        .update("Users", doc, ["age".to_string()].into_iter().collect())
        .await
        .unwrap();
}

/// Every block the document's heads reach, older versions included:
/// composites, field blocks and the encryption blocks they name.
async fn history<S>(db: &Arc<DB<S>>, doc_id: &DocID) -> RapidHashSet<Cid>
where
    S: Store + 'static,
{
    let txn = db.new_txn(true).await.unwrap();
    let doc_ref = db::docid::map::get_doc_ref(&txn.systemstore().unwrap(), &doc_id.to_string())
        .await
        .unwrap()
        .expect("document is stored");
    let heads = db::block::heads::document::load(&txn.headstore().unwrap(), doc_ref.doc_short_id)
        .await
        .unwrap();
    let blockstore = txn.blockstore().unwrap();
    let mut stack = db::block::heads::document::live_cids(&heads);
    let mut blocks = RapidHashSet::default();
    while let Some(cid) = stack.pop() {
        if !blocks.insert(cid) {
            continue;
        }
        let bytes = blockstore.get(&cid.to_bytes()).await.unwrap().unwrap();
        let block = defra_core::Block::from_dag_cbor(&bytes).unwrap();
        stack.extend(block.all_links());
        stack.extend(block.encryption);
    }
    drop(blockstore);
    let _ = txn.discard();
    blocks
}

async fn held<S>(db: &Arc<DB<S>>, cid: &Cid) -> bool
where
    S: Store + 'static,
{
    let txn = db.new_txn(true).await.unwrap();
    let held = txn
        .blockstore()
        .unwrap()
        .get(&cid.to_bytes())
        .await
        .unwrap()
        .is_some();
    let _ = txn.discard();
    held
}

async fn owners<S>(db: &Arc<DB<S>>, cid: &Cid) -> Vec<String>
where
    S: Store + 'static,
{
    let txn = db.new_txn(true).await.unwrap();
    let owners =
        db::docid::map::get_doc_ids_for_block(&txn.systemstore().unwrap(), &cid.to_string())
            .await
            .unwrap();
    let _ = txn.discard();
    owners
}

async fn users() -> Arc<DB<RegolithStore>> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(users_schema()).await.unwrap();
    db
}

#[test]
#[mantra_macros::req_test("go.shinzo.purge.prune_history")]
fn prune_deletes_history_and_keeps_a_block_another_document_owns() {
    workers::block_on(async {
        let db = users().await;
        let alice = create(&db, "Alice", 40).await;
        set_age(&db, &alice, 41).await;
        let carol = create(&db, "Alice", 7).await;
        let alice_blocks = history(&db, &alice).await;
        let carol_blocks = history(&db, &carol).await;
        assert_eq!(alice_blocks.len(), 5, "two composites, three field blocks");
        let shared: Vec<Cid> = alice_blocks.intersection(&carol_blocks).copied().collect();
        assert_eq!(shared.len(), 1, "the name block is shared");

        let purged = db
            .purge_by_doc_ids("Users", std::slice::from_ref(&alice), true, None)
            .await
            .unwrap();

        assert_eq!(purged, 1);
        for cid in alice_blocks.difference(&carol_blocks) {
            assert!(!held(&db, cid).await, "{cid} only Alice owned");
        }
        for cid in &carol_blocks {
            assert!(held(&db, cid).await, "{cid} is Carol's");
        }
        assert_eq!(owners(&db, &shared[0]).await, vec![carol.to_string()]);

        db.purge_by_doc_ids("Users", std::slice::from_ref(&carol), true, None)
            .await
            .unwrap();
        assert!(!held(&db, &shared[0]).await);
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.purge.prune_history")]
fn purging_every_owner_together_deletes_the_shared_block() {
    workers::block_on(async {
        let db = users().await;
        let alice = create(&db, "Alice", 40).await;
        set_age(&db, &alice, 41).await;
        let carol = create(&db, "Alice", 7).await;
        let blocks: RapidHashSet<Cid> = history(&db, &alice)
            .await
            .union(&history(&db, &carol).await)
            .copied()
            .collect();

        let purged = db
            .purge_by_doc_ids("Users", &[alice, carol], true, None)
            .await
            .unwrap();

        assert_eq!(purged, 2);
        for cid in &blocks {
            assert!(
                !held(&db, cid).await,
                "{cid} was owned only by purged documents"
            );
        }
    })
    .unwrap()
}

/// Go keeps the owner edges when it does not prune (`Go-Bug-Suspect` on
/// `go.shinzo.purge.mappings`), so the blocks stay owned by a document that
/// is gone. defradb.rs keeps the blocks and drops the edges.
#[test]
#[mantra_macros::req_test("go.shinzo.purge.mappings")]
fn purge_without_pruning_keeps_blocks_and_drops_the_owner_edges() {
    workers::block_on(async {
        let db = users().await;
        let alice = create(&db, "Alice", 40).await;
        let carol = create(&db, "Alice", 7).await;
        let blocks = history(&db, &alice).await;

        db.purge_by_doc_ids("Users", std::slice::from_ref(&alice), false, None)
            .await
            .unwrap();

        for cid in &blocks {
            assert!(held(&db, cid).await, "{cid} is kept without pruning");
            assert!(
                !owners(&db, cid).await.contains(&alice.to_string()),
                "{cid} still names the purged document"
            );
        }
        let txn = db.new_txn(true).await.unwrap();
        let systemstore = txn.systemstore().unwrap();
        assert!(
            db::docid::map::get_doc_ref(&systemstore, &alice.to_string())
                .await
                .unwrap()
                .is_none(),
            "the purged DocID no longer resolves"
        );
        assert!(
            db::docid::map::get_doc_ref(&systemstore, &carol.to_string())
                .await
                .unwrap()
                .is_some()
        );
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.purge.count")]
fn the_count_names_each_stored_document_once() {
    workers::block_on(async {
        let db = users().await;
        let alice = create(&db, "Alice", 40).await;
        let bob = create(&db, "Bob", 50).await;

        let purged = db
            .purge_by_doc_ids("Users", &[alice.clone(), bob, alice.clone()], false, None)
            .await
            .unwrap();
        assert_eq!(purged, 2);

        let again = db
            .purge_by_doc_ids("Users", &[alice], true, None)
            .await
            .unwrap();
        assert_eq!(again, 0, "a DocID that names nothing is skipped");
    })
    .unwrap()
}

/// The raw key and value a competitor commits.
type CompetingWrite = Arc<regolith::sync::Atom<Option<(Vec<u8>, Vec<u8>)>>>;

/// A store whose next `owed` retry attempts each see a competitor commit
/// `write` after the attempt ran and before it commits.
struct CompetingStore {
    inner: Arc<RegolithStore>,
    write: CompetingWrite,
    owed: Arc<AtomicUsize>,
    attempts: Arc<AtomicUsize>,
}

impl Sealed for CompetingStore {}

#[async_trait]
impl Store for CompetingStore {
    fn crdts(&self) -> &storage::Crdts {
        self.inner.crdts()
    }

    async fn allocate(&self, key: &[u8], count: u64) -> storage::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> KvResult<Box<dyn Txn>> {
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: RetryOptions,
        attempt: RetryAttempt,
    ) -> std::result::Result<RetryAttemptValue, RetryAttemptError> {
        let wrapped = Box::new(Competing {
            inner: attempt,
            store: Arc::clone(&self.inner),
            write: Arc::clone(&self.write),
            owed: Arc::clone(&self.owed),
            attempts: Arc::clone(&self.attempts),
        });
        self.inner.transact_retry(options, wrapped).await
    }

    async fn close(&self) -> KvResult<()> {
        self.inner.close().await
    }
}

struct Competing {
    inner: RetryAttempt,
    store: Arc<RegolithStore>,
    write: CompetingWrite,
    owed: Arc<AtomicUsize>,
    attempts: Arc<AtomicUsize>,
}

impl RetryAttemptFn for Competing {
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            let output = self.inner.run(retry).await?;
            let owed = self
                .owed
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                })
                .is_ok();
            let write = (*self.write.load()).clone();
            if let (true, Some((key, value))) = (owed, write) {
                let mut competitor = self.store.new_txn(false).await?;
                competitor.set(&key, &value).await?;
                competitor.commit().await?;
            }
            Ok(output)
        })
    }
}

fn namespaced(namespace: Namespace, key: &[u8]) -> Vec<u8> {
    let mut raw = vec![namespace.prefix()];
    raw.extend_from_slice(key);
    raw
}

async fn competing() -> (Arc<DB<CompetingStore>>, CompetingStore) {
    let inner = Arc::new(RegolithStore::in_memory().unwrap());
    let handles = || CompetingStore {
        inner: Arc::clone(&inner),
        write: Arc::new(regolith::sync::Atom::new(None)),
        owed: Arc::new(AtomicUsize::new(0)),
        attempts: Arc::new(AtomicUsize::new(0)),
    };
    let control = handles();
    let store = CompetingStore {
        inner: Arc::clone(&inner),
        write: Arc::clone(&control.write),
        owed: Arc::clone(&control.owed),
        attempts: Arc::clone(&control.attempts),
    };
    let db = Arc::new(DB::new(store).unwrap());
    db.create_collection(users_schema()).await.unwrap();
    (db, control)
}

#[test]
#[mantra_macros::req_test("go.shinzo.purge.no_conflict")]
fn a_sync_storing_a_walked_block_again_does_not_fail_the_purge() {
    workers::block_on(async {
        let (db, control) = competing().await;
        let alice = create(&db, "Alice", 40).await;
        let blocks = history(&db, &alice).await;
        let target = *blocks.iter().next().unwrap();
        let bytes = {
            let txn = db.new_txn(true).await.unwrap();
            let bytes = txn
                .blockstore()
                .unwrap()
                .get(&target.to_bytes())
                .await
                .unwrap()
                .unwrap();
            let _ = txn.discard();
            bytes.to_vec()
        };
        control.write.store(Some((
            namespaced(Namespace::Blockstore, &target.to_bytes()),
            bytes,
        )));
        control.owed.store(1, Ordering::SeqCst);
        control.attempts.store(0, Ordering::SeqCst);

        let purged = db
            .purge_by_doc_ids("Users", std::slice::from_ref(&alice), true, None)
            .await
            .unwrap();

        assert_eq!(purged, 1);
        for cid in &blocks {
            assert!(!held(&db, cid).await, "{cid} was Alice's alone");
        }
    })
    .unwrap()
}

/// A merge that adopts a stored field block writes only an owner edge. The
/// prune's owner scan is validated, so the attempt that decided on the old
/// owners conflicts, and the re-run keeps the block.
#[test]
#[mantra_macros::req_test("go.shinzo.purge.prune_history")]
fn a_block_a_merge_adopts_during_the_purge_is_kept() {
    workers::block_on(async {
        let (db, control) = competing().await;
        let alice = create(&db, "Alice", 40).await;
        let blocks = history(&db, &alice).await;
        let adopted = *blocks.iter().next().unwrap();
        let newcomer = "bae-0000000000000000000000000000000000000000000000000000000000000000";
        control.write.store(Some((
            namespaced(
                Namespace::Systemstore,
                &BlockCIDToDocIDKey::bytes_for(&adopted.to_string(), newcomer),
            ),
            newcomer.as_bytes().to_vec(),
        )));
        control.owed.store(1, Ordering::SeqCst);
        control.attempts.store(0, Ordering::SeqCst);

        let purged = db
            .purge_by_doc_ids("Users", std::slice::from_ref(&alice), true, None)
            .await
            .unwrap();

        assert_eq!(purged, 1);
        assert_eq!(
            control.attempts.load(Ordering::SeqCst),
            2,
            "the first attempt conflicted"
        );
        assert!(held(&db, &adopted).await, "the adopted block is kept");
        assert_eq!(owners(&db, &adopted).await, vec![newcomer.to_string()]);
    })
    .unwrap()
}

async fn branchable_users() -> Arc<DB<RegolithStore>> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(users_schema().as_branchable())
        .await
        .unwrap();
    db
}

/// The document composites the branchable collection's DAG links, from its
/// heads back through every collection block.
async fn collection_linked<S>(db: &Arc<DB<S>>) -> RapidHashSet<Cid>
where
    S: Store + 'static,
{
    let collection = db.get_collection("Users").unwrap().unwrap();
    let txn = db.new_txn(true).await.unwrap();
    let heads = db::block::heads::live_collection_heads(
        &txn.headstore().unwrap(),
        collection.resolved_root_id(),
    )
    .await
    .unwrap()
    .live;
    let blockstore = txn.blockstore().unwrap();
    let mut stack = heads;
    let mut seen = RapidHashSet::default();
    let mut linked = RapidHashSet::default();
    while let Some(cid) = stack.pop() {
        if !seen.insert(cid) {
            continue;
        }
        let bytes = blockstore.get(&cid.to_bytes()).await.unwrap().unwrap();
        let block = defra_core::Block::from_dag_cbor(&bytes).unwrap();
        linked.extend(block.links.iter().flatten().map(|link| link.link));
        stack.extend(block.heads.iter().flatten().copied());
    }
    drop(blockstore);
    let _ = txn.discard();
    linked
}

/// A branchable collection's DAG links every version of its documents, so
/// pruning a document's history keeps each composite the collection DAG
/// links, and every block those composites link: `_commits` of the
/// collection stays whole and a peer syncing it finds every block.
#[test]
#[mantra_macros::req_test("go.shinzo.purge.prune_history")]
fn prune_keeps_the_composites_a_branchable_collection_links() {
    workers::block_on(async {
        for in_txn in [false, true] {
            let db = branchable_users().await;
            let alice = create(&db, "Alice", 40).await;
            set_age(&db, &alice, 41).await;
            let blocks = history(&db, &alice).await;
            let linked = collection_linked(&db).await;
            assert_eq!(linked.len(), 2, "one collection block per version");
            assert!(linked.is_subset(&blocks));

            let purged = match in_txn {
                false => db
                    .purge_by_doc_ids("Users", std::slice::from_ref(&alice), true, None)
                    .await
                    .unwrap(),
                true => {
                    let registry = db::txn::registry::DbTransactionRegistry::new(db.clone());
                    let handle = registry.begin(false).await.unwrap();
                    let context = defra_core::RequestContext::default()
                        .with_txn(Some(handle.as_str().parse().unwrap()));
                    let purged = registry
                        .purge_by_doc_ids_as(&context, "Users", std::slice::from_ref(&alice), true)
                        .await
                        .unwrap();
                    registry.commit(&handle).await.unwrap();
                    purged
                }
            };

            assert_eq!(purged, 1);
            for cid in &blocks {
                assert!(
                    held(&db, cid).await,
                    "{cid} is linked by the collection DAG (txn {in_txn})"
                );
                assert!(
                    owners(&db, cid).await.is_empty(),
                    "{cid} no longer names Alice"
                );
            }
            assert_eq!(collection_linked(&db).await, linked);
        }
    })
    .unwrap()
}

/// A purge in a caller's transaction validates the collection's definition
/// as every write there does, so switching the collection's active version
/// while it runs makes the transaction's commit conflict.
#[test]
#[mantra_macros::req_test("go.shinzo.purge.count")]
fn a_purge_in_a_transaction_conflicts_with_a_definition_change() {
    workers::block_on(async {
        let db = users().await;
        let alice = create(&db, "Alice", 40).await;
        db.patch_collection(
            "Users",
            r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"extra","Kind":"String"}}]"#,
            None,
        )
        .await
        .unwrap();
        let registry = db::txn::registry::DbTransactionRegistry::new(db.clone());
        let handle = registry.begin(false).await.unwrap();
        let context =
            defra_core::RequestContext::default().with_txn(Some(handle.as_str().parse().unwrap()));
        registry
            .purge_by_doc_ids_as(&context, "Users", std::slice::from_ref(&alice), false)
            .await
            .unwrap();

        db.set_active_collection_version("users-v1").await.unwrap();

        let error = registry.commit(&handle).await.unwrap_err().to_string();
        assert!(error.contains("conflict"), "{error}");
    })
    .unwrap()
}

/// A truncate's chunks run as engine-retried attempts: a merge adopting a
/// block the chunk's owner scan decided on conflicts with the first attempt,
/// the chunk runs again, and the truncate completes with the block kept.
#[test]
#[mantra_macros::req_test("go.shinzo.purge.no_conflict")]
fn a_truncate_chunk_reruns_after_a_conflicting_owner_write() {
    workers::block_on(async {
        for filtered in [true, false] {
            let (db, control) = competing().await;
            let alice = create(&db, "Alice", 40).await;
            let adopted = *history(&db, &alice).await.iter().next().unwrap();
            let newcomer = "bae-0000000000000000000000000000000000000000000000000000000000000000";
            control.write.store(Some((
                namespaced(
                    Namespace::Systemstore,
                    &BlockCIDToDocIDKey::bytes_for(&adopted.to_string(), newcomer),
                ),
                newcomer.as_bytes().to_vec(),
            )));
            control.owed.store(1, Ordering::SeqCst);
            control.attempts.store(0, Ordering::SeqCst);

            let filter = filtered.then(|| {
                let mut conditions = serde_json::Map::new();
                conditions.insert("name".into(), serde_json::json!({ "_eq": "Alice" }));
                query::documents::Filter::Conditions(conditions)
            });
            db.truncate_collection_as(
                &defra_core::RequestContext::ambient(),
                "Users",
                filter.as_ref(),
            )
            .await
            .unwrap();

            assert!(
                control.attempts.load(Ordering::SeqCst) >= 2,
                "the chunk ran as retried attempts and re-ran (filtered {filtered})"
            );
            assert!(
                held(&db, &adopted).await,
                "the adopted block is kept (filtered {filtered})"
            );
            assert_eq!(owners(&db, &adopted).await, vec![newcomer.to_string()]);
        }
    })
    .unwrap()
}

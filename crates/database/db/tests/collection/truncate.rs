use crate::common::schema::parsed;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use db::{
    AutoCommitFetcher, AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry,
    LensedAutoCommitFetcher, TruncateError, DB,
};
use defra_core::{RequestContext, TxnId};
use document::{DocID, Document, NormalValue};
use query::documents::Filter;
use query::{DocFetcher, DocMutator, QueryExecutor, QueryRequest};
use schema::CollectionVersion;
use storage::corekv::{IterOptions, Key, RetryAttemptFuture};
use storage::keys::{DatastoreSE, IndexDataStoreKey};
use storage::RegolithStore;

const USERS_SDL: &str =
    r#"type Users { name: String @index(name: "users_name", unique: true) age: Int }"#;

fn users_schema() -> CollectionVersion {
    parsed(USERS_SDL)
}

fn age_filter(age: i64) -> Filter {
    serde_json::json!({"age": {"_eq": age}}).into()
}

async fn create_user(mutator: &AutoCommitMutator<RegolithStore>, name: &str, age: i64) -> DocID {
    let mut doc = Document::new(users_schema());
    doc.set("name", NormalValue::String(name.to_owned()))
        .unwrap();
    doc.set("age", NormalValue::Int(age)).unwrap();
    mutator.create("Users", doc).await.unwrap().doc_id
}

async fn set_age(db: &Arc<DB<RegolithStore>>, doc_id: &DocID, age: i64) {
    let mut doc = Document::new(users_schema());
    doc.set_id(doc_id.clone());
    doc.set("age", NormalValue::Int(age)).unwrap();
    AutoCommitMutator::new(Arc::clone(db))
        .update("Users", doc, ["age".to_string()].into_iter().collect())
        .await
        .unwrap();
}

type RacingWriteFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
type RacingWrite = Arc<dyn Fn() -> RacingWriteFuture + Send + Sync>;

/// Commits `race` once, after the first attempt has read and before it
/// commits.
struct FilterMovingStore {
    inner: Arc<RegolithStore>,
    race: RacingWrite,
    armed: Arc<AtomicBool>,
    attempts: Arc<AtomicUsize>,
}

impl storage::corekv::private::Sealed for FilterMovingStore {}

#[async_trait::async_trait]
impl storage::corekv::Store for FilterMovingStore {
    fn crdts(&self) -> &storage::Crdts {
        self.inner.crdts()
    }

    async fn allocate(
        &self,
        key: &[u8],
        count: u64,
    ) -> storage::corekv::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(
        &self,
        readonly: bool,
    ) -> storage::corekv::Result<Box<dyn storage::corekv::Txn>> {
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: storage::corekv::RetryOptions,
        attempt: storage::corekv::RetryAttempt,
    ) -> std::result::Result<storage::corekv::RetryAttemptValue, storage::corekv::RetryAttemptError>
    {
        let attempt = Box::new(FilterMovingAttempt {
            inner: attempt,
            race: Arc::clone(&self.race),
            armed: Arc::clone(&self.armed),
            attempts: Arc::clone(&self.attempts),
        });
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> storage::corekv::Result<()> {
        self.inner.close().await
    }
}

struct FilterMovingAttempt {
    inner: storage::corekv::RetryAttempt,
    race: RacingWrite,
    armed: Arc<AtomicBool>,
    attempts: Arc<AtomicUsize>,
}

impl storage::corekv::RetryAttemptFn for FilterMovingAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn storage::corekv::RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            let output = self.inner.run(txn).await?;
            if self.armed.swap(false, Ordering::SeqCst) {
                (self.race)().await;
            }
            Ok(output)
        })
    }
}

#[test]
fn a_conflicted_filtered_truncate_rereads_its_matches() {
    workers::block_on(async {
        let inner = Arc::new(RegolithStore::in_memory().unwrap());
        let racer = Arc::new(DB::open((*inner).clone()).await.unwrap());
        racer
            .add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
            .await
            .unwrap();
        let alice = create_user(&AutoCommitMutator::new(Arc::clone(&racer)), "Alice", 30).await;
        let armed = Arc::new(AtomicBool::new(true));
        let attempts = Arc::new(AtomicUsize::new(0));
        let race: RacingWrite = {
            let racer = Arc::clone(&racer);
            Arc::new(move || {
                let (racer, alice) = (Arc::clone(&racer), alice.clone());
                Box::pin(async move { set_age(&racer, &alice, 31).await })
            })
        };
        let store = FilterMovingStore {
            inner: Arc::clone(&inner),
            race,
            armed: Arc::clone(&armed),
            attempts: Arc::clone(&attempts),
        };
        let db = Arc::new(DB::open(store).await.unwrap());

        db.truncate_collection_as(&RequestContext::ambient(), "Users", Some(&age_filter(30)))
            .await
            .unwrap();

        assert!(
            attempts.load(Ordering::SeqCst) >= 2,
            "the first page conflicted and ran again"
        );
        let docs = AutoCommitFetcher::new(db).get_all("Users").await.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].get("age"), Some(&NormalValue::Int(31)));
    })
    .unwrap()
}

async fn run(queries: &dyn QueryExecutor, request: &str) -> serde_json::Value {
    let txn = queries.begin_txn(false).await.unwrap();
    let response = queries
        .execute_in_txn(QueryRequest::new(request), &txn)
        .await;
    assert!(!response.has_errors(), "{request}: {:?}", response.errors);
    queries.commit_txn(&txn).await.unwrap();
    response.data.unwrap()
}

#[test]
fn a_filtered_truncate_rereads_after_a_related_document_leaves_the_filter() {
    workers::block_on(async {
        let inner = Arc::new(RegolithStore::in_memory().unwrap());
        let racer = Arc::new(DB::open((*inner).clone()).await.unwrap());
        let ctx = RequestContext::ambient();
        racer
            .add_collection(&ctx, "type Author { name: String }")
            .await
            .unwrap();
        racer
            .add_collection(&ctx, "type Book { title: String author: Author }")
            .await
            .unwrap();
        let queries: Arc<dyn QueryExecutor> =
            Arc::new(query::QueryRunner::with_arc_registry_and_provider(
                LensedAutoCommitFetcher::new(Arc::clone(&racer)),
                DbCollectionProvider::new_arc(Arc::clone(&racer)),
                Arc::new(DbTransactionRegistry::new(Arc::clone(&racer))),
            ));
        let created = run(
            queries.as_ref(),
            r#"mutation { add_Author(input: {name: "Alice"}) { _docID } }"#,
        )
        .await;
        let author = created["add_Author"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned();
        run(
            queries.as_ref(),
            &format!(
                r#"mutation {{ add_Book(input: {{title: "Kept", author: "{author}"}}) {{ _docID }} }}"#
            ),
        )
        .await;

        let rename = format!(
            r#"mutation {{ update_Author(docID: "{author}", input: {{name: "Bob"}}) {{ _docID }} }}"#
        );
        let race: RacingWrite = {
            let queries = Arc::clone(&queries);
            Arc::new(move || {
                let (queries, rename) = (Arc::clone(&queries), rename.clone());
                Box::pin(async move {
                    run(queries.as_ref(), &rename).await;
                })
            })
        };
        let attempts = Arc::new(AtomicUsize::new(0));
        let store = FilterMovingStore {
            inner: Arc::clone(&inner),
            race,
            armed: Arc::new(AtomicBool::new(true)),
            attempts: Arc::clone(&attempts),
        };
        let db = Arc::new(DB::open(store).await.unwrap());
        let by_alice: Filter = serde_json::json!({"author": {"name": {"_eq": "Alice"}}}).into();

        db.truncate_collection_as(&ctx, "Book", Some(&by_alice))
            .await
            .unwrap();

        assert!(
            attempts.load(Ordering::SeqCst) >= 2,
            "the rename conflicted with the page's selection"
        );
        let books = run(queries.as_ref(), "query { Book { title } }").await;
        assert_eq!(books["Book"], serde_json::json!([{"title": "Kept"}]));
    })
    .unwrap()
}

#[test]
fn filtered_truncate_includes_soft_deleted_documents() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
            .await
            .unwrap();
        let mutator = AutoCommitMutator::new(db.clone());

        let mut doc = Document::new(users_schema());
        doc.set("name", NormalValue::String("Alice".into()))
            .unwrap();
        doc.set("age", NormalValue::Int(30)).unwrap();
        let created = mutator.create("Users", doc).await.unwrap();
        mutator.delete("Users", &created.doc_id).await.unwrap();

        db.truncate_collection_as(&RequestContext::ambient(), "Users", Some(&age_filter(30)))
            .await
            .unwrap();
        create_user(&mutator, "Alice", 31).await;

        assert_eq!(
            AutoCommitFetcher::new(db)
                .get_all("Users")
                .await
                .unwrap()
                .len(),
            1
        );
    })
    .unwrap()
}

#[test]
fn filtered_truncate_removes_only_the_matches_of_a_branchable_collection() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), r#"type Users @branchable { name: String @index(name: "users_name", unique: true) age: Int }"#)
            .await
            .unwrap();
        let mutator = AutoCommitMutator::new(db.clone());
        create_user(&mutator, "Alice", 30).await;
        create_user(&mutator, "Bob", 40).await;

        db.truncate_collection_as(&RequestContext::ambient(), "Users", Some(&age_filter(30)))
            .await
            .unwrap();
        create_user(&mutator, "Alice", 31).await;

        assert_eq!(
            AutoCommitFetcher::new(db)
                .get_all("Users")
                .await
                .unwrap()
                .len(),
            2
        );
    })
    .unwrap()
}

#[test]
fn filtered_truncate_removes_only_matching_searchable_encryption_records() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
            .await
            .unwrap();
        let mutator = AutoCommitMutator::new(db.clone());
        let alice = create_user(&mutator, "Alice", 30).await;
        let bob = create_user(&mutator, "Bob", 40).await;
        let alice_key = DatastoreSE::new(
            users_schema().collection_id,
            "users-name",
            vec![1],
            alice.to_string(),
        )
        .bytes();
        let bob_key = DatastoreSE::new(
            users_schema().collection_id,
            "users-name",
            vec![2],
            bob.to_string(),
        )
        .bytes();

        let txn = db.new_txn(false).await.unwrap();
        let datastore = txn.datastore().unwrap();
        datastore.set(&alice_key, b"alice").await.unwrap();
        datastore.set(&bob_key, b"bob").await.unwrap();
        drop(datastore);
        txn.commit().await.unwrap();

        db.truncate_collection_as(&RequestContext::ambient(), "Users", Some(&age_filter(30)))
            .await
            .unwrap();

        let txn = db.new_txn(true).await.unwrap();
        let datastore = txn.datastore().unwrap();
        assert!(datastore.get(&alice_key).await.unwrap().is_none());
        assert!(datastore.get(&bob_key).await.unwrap().is_some());
    })
    .unwrap()
}

#[test]
fn unfiltered_truncate_removes_searchable_encryption_records() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
            .await
            .unwrap();
        let mutator = AutoCommitMutator::new(db.clone());
        let alice = create_user(&mutator, "Alice", 30).await;
        let alice_key = DatastoreSE::new(
            users_schema().collection_id,
            "users-name",
            vec![1],
            alice.to_string(),
        )
        .bytes();
        // A replicator keeps artifacts for documents it does not hold.
        let unheld_key = DatastoreSE::new(
            users_schema().collection_id,
            "users-name",
            vec![2],
            "bae-unheld",
        )
        .bytes();
        let other_key =
            DatastoreSE::new("other-collection", "users-name", vec![3], "bae-other").bytes();

        let txn = db.new_txn(false).await.unwrap();
        let datastore = txn.datastore().unwrap();
        for key in [&alice_key, &unheld_key, &other_key] {
            datastore.set(key, b"").await.unwrap();
        }
        drop(datastore);
        txn.commit().await.unwrap();

        db.truncate_collection_as(&RequestContext::ambient(), "Users", None)
            .await
            .unwrap();

        let txn = db.new_txn(true).await.unwrap();
        let datastore = txn.datastore().unwrap();
        assert!(datastore.get(&alice_key).await.unwrap().is_none());
        assert!(datastore.get(&unheld_key).await.unwrap().is_none());
        assert!(datastore.get(&other_key).await.unwrap().is_some());
    })
    .unwrap()
}

#[test]
fn filtered_truncate_processes_more_than_one_chunk() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let schema_sdl = "type Users { age: Int seq: Int }";
        let schema = parsed(schema_sdl);
        db.add_collection(&defra_core::RequestContext::ambient(), schema_sdl)
            .await
            .unwrap();
        let mutator = AutoCommitMutator::new(db.clone());
        for seq in 0..=1000 {
            let mut doc = Document::new(schema.clone());
            doc.set("age", NormalValue::Int(30)).unwrap();
            doc.set("seq", NormalValue::Int(seq)).unwrap();
            mutator.create("Users", doc).await.unwrap();
        }

        db.truncate_collection_as(&RequestContext::ambient(), "Users", Some(&age_filter(30)))
            .await
            .unwrap();

        assert!(AutoCommitFetcher::new(db)
            .get_all("Users")
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap()
}

#[test]
fn filtered_truncate_in_a_transaction_is_refused_and_removes_nothing() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
            .await
            .unwrap();
        create_user(&AutoCommitMutator::new(db.clone()), "Alice", 30).await;
        let in_txn = RequestContext::ambient().with_txn(Some(TxnId { start: 0, route: 1 }));

        let error = db
            .truncate_collection_as(&in_txn, "Users", Some(&age_filter(30)))
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "filtered truncate cannot run in a transaction"
        );
        assert_eq!(
            AutoCommitFetcher::new(db)
                .get_all("Users")
                .await
                .unwrap()
                .len(),
            1
        );
    })
    .unwrap()
}

#[test]
fn unfiltered_truncate_in_a_transaction_applies_at_once() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
            .await
            .unwrap();
        create_user(&AutoCommitMutator::new(db.clone()), "Alice", 30).await;
        let in_txn = RequestContext::ambient().with_txn(Some(TxnId { start: 0, route: 1 }));

        db.truncate_collection_as(&in_txn, "Users", None)
            .await
            .unwrap();

        assert!(AutoCommitFetcher::new(db)
            .get_all("Users")
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap()
}

#[test]
fn a_filter_that_is_not_one_is_refused_with_go_text() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
            .await
            .unwrap();
        create_user(&AutoCommitMutator::new(db.clone()), "Alice", 30).await;

        for (filter, expected) in [
            (Filter::Source("  ".into()), "filter cannot be empty"),
            (
                Filter::from(serde_json::json!(4)),
                "unsupported filter type",
            ),
            (
                Filter::from(serde_json::json!(null)),
                "unsupported filter type",
            ),
        ] {
            let error = db
                .truncate_collection_as(&RequestContext::ambient(), "Users", Some(&filter))
                .await
                .unwrap_err();
            assert_eq!(error.to_string(), expected);
        }
        assert_eq!(
            AutoCommitFetcher::new(db)
                .get_all("Users")
                .await
                .unwrap()
                .len(),
            1
        );
    })
    .unwrap()
}

#[test]
fn a_source_text_filter_truncates_what_it_matches() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
            .await
            .unwrap();
        let mutator = AutoCommitMutator::new(db.clone());
        create_user(&mutator, "Alice", 30).await;
        create_user(&mutator, "Bob", 40).await;

        db.truncate_collection_as(
            &RequestContext::ambient(),
            "Users",
            Some(&Filter::Source("{age: {_eq: 30}}".into())),
        )
        .await
        .unwrap();

        let docs = AutoCommitFetcher::new(db).get_all("Users").await.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].get("name"),
            Some(&NormalValue::String("Bob".into()))
        );
    })
    .unwrap()
}

#[test]
fn truncating_a_missing_collection_names_it() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());

        let error = db
            .truncate_collection_as(&RequestContext::ambient(), "Users", None)
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            TruncateError::Db(db::Error::CollectionNotFound(name)) if name == "Users"
        ));
    })
    .unwrap()
}

fn named_schema_sdl(name: &str) -> String {
    format!(r#"type {name} {{ name: String @index(name: "{name}_name") }}"#)
}

fn named_schema(name: &str) -> CollectionVersion {
    parsed(&named_schema_sdl(name))
}

async fn create_named(db: &Arc<DB<RegolithStore>>, collection: &str) {
    let mut doc = Document::new(named_schema(collection));
    doc.set("name", NormalValue::String(collection.to_owned()))
        .unwrap();
    AutoCommitMutator::new(db.clone())
        .create(collection, doc)
        .await
        .unwrap();
}

async fn datastore_keys(db: &Arc<DB<RegolithStore>>, prefix: &[u8]) -> usize {
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iter = datastore
        .iterator(IterOptions::new().with_prefix(prefix.to_vec()))
        .await
        .unwrap();
    let mut count = 0;
    while iter.next().await.unwrap().is_some() {
        count += 1;
    }
    iter.close().await.unwrap();
    count
}

/// The entries of `collection`'s one index.
async fn index_keys(db: &Arc<DB<RegolithStore>>, collection: &str) -> usize {
    let collection = db.get_collection(collection).unwrap().unwrap();
    let index = &collection.get_indexes()[0];
    datastore_keys(
        db,
        &IndexDataStoreKey::index_prefix(collection.resolved_root_id(), index.id),
    )
    .await
}

/// A collection whose short ID is written as the byte of a family other
/// collections keep their keys in (100 is `d`, the documents; 118 is `v`,
/// their schema versions) is truncated without touching those keys, and its
/// own index entries are gone.
#[test]
fn truncating_a_collection_whose_short_id_spells_a_family_keeps_other_collections() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.add_collection(
            &defra_core::RequestContext::ambient(),
            &named_schema_sdl("Kept"),
        )
        .await
        .unwrap();
        create_named(&db, "Kept").await;
        let mut spelled = Vec::new();
        for n in 0..200 {
            let name = format!("Filler{n}");
            db.add_collection(
                &defra_core::RequestContext::ambient(),
                &named_schema_sdl(&name),
            )
            .await
            .unwrap();
            let short_id = db
                .get_collection(&name)
                .unwrap()
                .unwrap()
                .resolved_root_id();
            if short_id == u32::from(b'd') || short_id == u32::from(b'v') {
                create_named(&db, &name).await;
                spelled.push(name);
            }
            if short_id > u32::from(b'v') {
                break;
            }
        }
        assert_eq!(spelled.len(), 2, "no collection got short ID 100 or 118");
        let kept_id = named_schema("Kept").collection_id;
        let kept_documents = datastore_keys(&db, format!("/d/{kept_id}/").as_bytes()).await;
        let kept_versions = datastore_keys(&db, format!("/v/{kept_id}/").as_bytes()).await;
        let kept_entries = index_keys(&db, "Kept").await;
        assert!(kept_documents > 0 && kept_versions > 0 && kept_entries > 0);

        for name in &spelled {
            assert!(index_keys(&db, name).await > 0);
            db.truncate_collection_as(&RequestContext::ambient(), name, None)
                .await
                .unwrap();
            assert_eq!(index_keys(&db, name).await, 0);
            assert!(AutoCommitFetcher::new(db.clone())
                .get_all(name)
                .await
                .unwrap()
                .is_empty());
        }

        assert_eq!(
            datastore_keys(&db, format!("/d/{kept_id}/").as_bytes()).await,
            kept_documents
        );
        assert_eq!(
            datastore_keys(&db, format!("/v/{kept_id}/").as_bytes()).await,
            kept_versions
        );
        assert_eq!(index_keys(&db, "Kept").await, kept_entries);
        assert_eq!(
            AutoCommitFetcher::new(db)
                .get_all("Kept")
                .await
                .unwrap()
                .len(),
            1
        );
    })
    .unwrap()
}

/// Truncate removes a document's DocID mapping and its blocks' ownership:
/// before it the genesis field block is owned by the DocID, after it the
/// DocID resolves to nothing and the block has no owner.
#[test]
#[mantra_macros::req_test("defra.collection.truncate_ownership")]
fn truncate_removes_doc_id_mappings_and_block_ownership() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
            .await
            .unwrap();
        let mut doc = Document::new(users_schema());
        doc.set("name", NormalValue::String("Alice".to_owned()))
            .unwrap();
        doc.set("age", NormalValue::Int(30)).unwrap();
        let created = AutoCommitMutator::new(db.clone())
            .create("Users", doc)
            .await
            .unwrap();
        let composite =
            defra_core::Block::from_dag_cbor(created.commit_block.as_ref().unwrap()).unwrap();
        let field = composite.links.unwrap()[0].link.to_string();
        let doc_id = created.doc_id.to_string();

        let lookups = || {
            let db = db.clone();
            let (field, doc_id) = (field.clone(), doc_id.clone());
            async move {
                let txn = db.new_txn(true).await.unwrap();
                let systemstore = txn.systemstore().unwrap();
                let owners = db::docid::map::get_doc_ids_for_block(&systemstore, &field)
                    .await
                    .unwrap();
                let resolves = db::docid::map::get_doc_ref(&systemstore, &doc_id)
                    .await
                    .unwrap()
                    .is_some();
                drop(systemstore);
                let _ = txn.discard();
                (owners, resolves)
            }
        };
        assert_eq!(lookups().await, (vec![doc_id.clone()], true));
        db.truncate_collection_as(&RequestContext::ambient(), "Users", None)
            .await
            .unwrap();
        assert_eq!(lookups().await, (Vec::<String>::new(), false));
    })
    .unwrap()
}

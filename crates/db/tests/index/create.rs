//! `DB::create_index`: the definition waits on the collection write guard,
//! the documents that already exist are indexed in batches that hold no
//! guard, writers landing meanwhile keep the index exact, a batch that read
//! a document that changed underneath aborts, and a restart resumes an
//! interrupted backfill.

use rapidhash::RapidHashSet;
use std::sync::Arc;

use db::index::backfill::{encode_progress, BACKFILL_BATCH_DOCS};
use db::{AutoCommitMutator, BackfillSource, Collection, IndexManager, DB};
use document::{DocID, Document, NormalValue};
use query::DocMutator;
use schema::{
    CollectionVersion, FieldDescription, FieldKind, IndexKind, IndexedFieldDescription,
    OrderedIndexDescription,
};
use storage::corekv::{IterOptions, Iterator, Key};
use storage::index::IndexIterator;
use storage::keys::datastore::IndexDataStoreKey;
use storage::keys::systemstore::{ActionProgressKey, CollectionKey, CollectionNameKey};
use storage::RegolithStore;

use crate::common::guard_events::{recorder, WRITE_HOLDING, WRITE_WAITING};

fn users_schema(name: &str, version_id: &str, collection_id: &str) -> CollectionVersion {
    CollectionVersion::new(
        name,
        version_id,
        collection_id,
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "name", FieldKind::string()),
        ],
    )
}

fn by_name() -> (Vec<IndexedFieldDescription>, IndexKind) {
    (
        vec![IndexedFieldDescription {
            name: "name".to_string(),
            descending: false,
        }],
        IndexKind::Ordered(OrderedIndexDescription { unique: false }),
    )
}

async fn open_with_collection(
    name: &str,
    version_id: &str,
    collection_id: &str,
) -> (Arc<RegolithStore>, Arc<DB<RegolithStore>>) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collection(users_schema(name, version_id, collection_id))
        .await
        .unwrap();
    (store, db)
}

async fn create_user(
    mutator: &AutoCommitMutator<RegolithStore>,
    collection: &str,
    name: &str,
) -> DocID {
    let mut doc = Document::new();
    doc.set("name", NormalValue::String(name.to_owned()));
    mutator.create(collection, doc).await.unwrap().doc_id
}

/// Run `attempt` until it succeeds or fails for a reason other than a
/// transaction conflict, which a write racing a backfill batch may hit once.
async fn retrying<T, F, Fut>(mut attempt: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = query::error::Result<T>>,
{
    let mut tries = 0;
    loop {
        match attempt().await {
            Ok(value) => return value,
            Err(error) if error.to_string().contains("transaction conflict") && tries < 32 => {
                tries += 1
            }
            Err(error) => panic!("write failed for a reason other than a conflict: {error}"),
        }
    }
}

async fn index_entries(db: &DB<RegolithStore>, collection: &str, index_id: u32) -> usize {
    let short_id = db
        .require_collection(collection)
        .unwrap()
        .schema()
        .resolved_root_id();
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iter = datastore
        .iterator(
            IterOptions::new().with_prefix(IndexDataStoreKey::index_prefix(short_id, index_id)),
        )
        .await
        .unwrap();
    let mut count = 0;
    while iter.next().await.unwrap().is_some() {
        count += 1;
    }
    count
}

async fn entries_for(db: &DB<RegolithStore>, collection: &str, name: &str) -> usize {
    let collection = db.require_collection(collection).unwrap();
    let manager =
        IndexManager::from_collection(collection.schema().resolved_root_id(), collection.schema())
            .unwrap();
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut entries = manager
        .get_index("by_name")
        .unwrap()
        .get(&datastore, &[NormalValue::String(name.to_string())])
        .await
        .unwrap();
    let mut count = 0;
    while entries.next().await.unwrap().is_some() {
        count += 1;
    }
    count
}

async fn progress_recorded(db: &DB<RegolithStore>, collection_id: &str, index_id: u32) -> bool {
    let txn = db.new_txn(true).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    systemstore
        .has(
            &ActionProgressKey::new(
                collection_id,
                defra_core::Action::BACKFILL_INDEX,
                index_id.to_string(),
            )
            .bytes(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn create_index_indexes_the_documents_that_already_exist() {
    let (_store, db) =
        open_with_collection("Backfilled", "backfilled-v1", "col-index-backfill").await;
    let mutator = AutoCommitMutator::new(db.clone());
    for name in ["ada", "grace", "linus"] {
        create_user(&mutator, "Backfilled", name).await;
    }

    let (fields, kind) = by_name();
    let index = db
        .create_index("Backfilled", Some("by_name"), fields, kind)
        .await
        .unwrap();

    // A completed backfill clears its action; only an errored or running one
    // is listed.
    let actions = db.list_index_actions("col-index-backfill").await.unwrap();
    assert!(
        !actions.contains_key(&index.id),
        "the backfill did not complete: {:?}",
        actions.get(&index.id)
    );
    assert!(!progress_recorded(&db, "col-index-backfill", index.id).await);
    assert_eq!(index_entries(&db, "Backfilled", index.id).await, 3);
    assert!(
        db.require_collection("Backfilled")
            .unwrap()
            .get_indexes()
            .iter()
            .any(|existing| existing.name == "by_name"),
        "the definition must be on the cached collection"
    );
}

#[tokio::test]
async fn create_index_backfills_more_documents_than_a_batch_holds() {
    let total = 2 * BACKFILL_BATCH_DOCS + 37;
    let (_store, db) = open_with_collection("Batched", "batched-v1", "col-index-batched").await;
    let mutator = AutoCommitMutator::new(db.clone());
    for i in 0..total {
        create_user(&mutator, "Batched", &format!("user-{i}")).await;
    }

    let (fields, kind) = by_name();
    let index = db
        .create_index("Batched", Some("by_name"), fields, kind)
        .await
        .unwrap();

    assert_eq!(index_entries(&db, "Batched", index.id).await, total);
    assert!(!db
        .list_index_actions("col-index-batched")
        .await
        .unwrap()
        .contains_key(&index.id));
    assert!(!progress_recorded(&db, "col-index-batched", index.id).await);
    assert_eq!(entries_for(&db, "Batched", "user-0").await, 1);
    assert_eq!(
        entries_for(&db, "Batched", &format!("user-{}", total - 1)).await,
        1
    );
}

/// Stand in for a truncate, delete or patch by holding the collection write
/// guard; the definition must wait on it, write nothing meanwhile, and land
/// once it is released. The guard events are counted per collection: the
/// test's own acquisition is the first waiting and holding pair.
#[tokio::test]
async fn create_index_waits_for_the_collection_write_guard() {
    const COLLECTION_ID: &str = "col-index-guard";
    let recorder = recorder();
    let (_store, db) = open_with_collection("Guarded", "guarded-v1", COLLECTION_ID).await;
    create_user(&AutoCommitMutator::new(db.clone()), "Guarded", "ada").await;

    let guards = db
        .collection_write_guards(std::iter::once(COLLECTION_ID.to_string()))
        .await
        .unwrap();
    assert_eq!(recorder.count(COLLECTION_ID, WRITE_HOLDING), 1);

    let creator = db.clone();
    let mut task = tokio::spawn(async move {
        let (fields, kind) = by_name();
        creator
            .create_index("Guarded", Some("by_name"), fields, kind)
            .await
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while recorder.count(COLLECTION_ID, WRITE_WAITING) < 2 {
        if task.is_finished() {
            let outcome = (&mut task).await;
            panic!("index creation ended without waiting on the collection guard: {outcome:?}");
        }
        assert!(
            std::time::Instant::now() < deadline,
            "index creation never reached the collection guard"
        );
        tokio::task::yield_now().await;
    }
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }

    assert_eq!(
        recorder.count(COLLECTION_ID, WRITE_HOLDING),
        1,
        "index creation must not acquire the collection guard while it is held"
    );
    assert!(!task.is_finished());
    assert!(
        db.require_collection("Guarded")
            .unwrap()
            .get_indexes()
            .is_empty(),
        "no definition may land while the guard is held"
    );

    drop(guards);

    let index = task
        .await
        .expect("index creation panicked")
        .expect("index creation should succeed once the guard is released");
    assert_eq!(recorder.count(COLLECTION_ID, WRITE_HOLDING), 2);
    assert_eq!(index_entries(&db, "Guarded", index.id).await, 1);
}

/// Writers run through the whole backfill: creates, updates of documents the
/// batches have not reached yet and of ones they have, deletes. Afterwards
/// every live document has exactly one entry, for its current value, and
/// no other entry exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_index_keeps_pace_with_concurrent_writers() {
    let existing = 3 * BACKFILL_BATCH_DOCS;
    let (_store, db) = open_with_collection("Racing", "racing-v1", "col-index-racing").await;
    let mutator = Arc::new(AutoCommitMutator::new(db.clone()));
    let mut ids = Vec::with_capacity(existing);
    for i in 0..existing {
        ids.push(create_user(&mutator, "Racing", &format!("pre-{i}")).await);
    }
    let updated = 0..200;
    let deleted = 500..600;
    let created = 0..200;

    let creator = db.clone();
    let index_task = tokio::spawn(async move {
        let (fields, kind) = by_name();
        creator
            .create_index("Racing", Some("by_name"), fields, kind)
            .await
    });
    let updater = {
        let mutator = mutator.clone();
        let ids = ids.clone();
        let updated = updated.clone();
        tokio::spawn(async move {
            for i in updated {
                retrying(|| async {
                    let mut doc = Document::new();
                    doc.set_id(ids[i].clone());
                    doc.set("name", NormalValue::String(format!("upd-{i}")));
                    mutator
                        .update("Racing", doc, RapidHashSet::from_iter(["name".to_string()]))
                        .await
                })
                .await;
            }
        })
    };
    let deleter = {
        let mutator = mutator.clone();
        let ids = ids.clone();
        let deleted = deleted.clone();
        tokio::spawn(async move {
            for i in deleted {
                retrying(|| mutator.delete("Racing", &ids[i])).await;
            }
        })
    };
    let inserter = {
        let mutator = mutator.clone();
        let created = created.clone();
        tokio::spawn(async move {
            for i in created {
                retrying(|| async {
                    let mut doc = Document::new();
                    doc.set("name", NormalValue::String(format!("new-{i}")));
                    mutator.create("Racing", doc).await
                })
                .await;
            }
        })
    };
    let index = index_task.await.unwrap().unwrap();
    updater.await.unwrap();
    deleter.await.unwrap();
    inserter.await.unwrap();

    let actions = db.list_index_actions("col-index-racing").await.unwrap();
    assert!(
        !actions.contains_key(&index.id),
        "the backfill did not complete: {:?}",
        actions.get(&index.id)
    );
    for i in 0..existing {
        let (expect_pre, expect_upd) = if deleted.contains(&i) {
            (0, 0)
        } else if updated.contains(&i) {
            (0, 1)
        } else {
            (1, 0)
        };
        assert_eq!(
            entries_for(&db, "Racing", &format!("pre-{i}")).await,
            expect_pre,
            "pre-{i}"
        );
        assert_eq!(
            entries_for(&db, "Racing", &format!("upd-{i}")).await,
            expect_upd,
            "upd-{i}"
        );
    }
    for i in created.clone() {
        assert_eq!(entries_for(&db, "Racing", &format!("new-{i}")).await, 1);
    }
    assert_eq!(
        index_entries(&db, "Racing", index.id).await,
        existing - deleted.len() + created.len()
    );
    assert!(!db
        .list_index_actions("col-index-racing")
        .await
        .unwrap()
        .contains_key(&index.id));
}

/// The batch scanned the document and observed it, so the commit sees the
/// update that landed in between and refuses rather than keep an entry for
/// a value the document no longer has.
#[tokio::test]
async fn backfill_batch_aborts_when_a_document_it_read_changes() {
    let (_store, db) = open_with_collection("Observed", "observed-v1", "col-index-observed").await;
    let mutator = AutoCommitMutator::new(db.clone());
    let mut ids = Vec::new();
    for name in ["ada", "grace", "linus"] {
        ids.push(create_user(&mutator, "Observed", name).await);
    }
    let (fields, kind) = by_name();
    db.create_index("Observed", Some("by_name"), fields, kind)
        .await
        .unwrap();

    let collection = db.require_collection("Observed").unwrap();
    let manager =
        IndexManager::from_collection(collection.schema().resolved_root_id(), collection.schema())
            .unwrap();
    let txn = db.new_txn(false).await.unwrap();
    {
        let datastore = txn.datastore().unwrap();
        let systemstore = txn.systemstore().unwrap();
        let mut source = BackfillSource::open_range(
            collection.clone(),
            datastore.clone(),
            systemstore.clone(),
            None,
            None,
        )
        .await
        .unwrap();
        let batch = manager
            .index_batch_from(
                &datastore,
                "by_name",
                &mut source,
                collection.schema(),
                usize::MAX,
            )
            .await
            .unwrap();
        assert_eq!(batch.indexed, 3);
        assert!(batch.exhausted);
    }

    let mut doc = Document::new();
    doc.set_id(ids[1].clone());
    doc.set("name", NormalValue::String("grace-hopper".to_string()));
    mutator
        .update(
            "Observed",
            doc,
            RapidHashSet::from_iter(["name".to_string()]),
        )
        .await
        .unwrap();

    let error = txn
        .commit()
        .await
        .expect_err("the batch read a document that changed underneath");
    assert!(error.is_txn_conflict(), "{error}");
    assert_eq!(entries_for(&db, "Observed", "grace").await, 0);
    assert_eq!(entries_for(&db, "Observed", "grace-hopper").await, 1);
}

/// The definition, its in-progress action and one committed batch survive
/// the process; opening the store again finishes the backfill from the
/// batch after the durable one and clears the action.
#[tokio::test]
async fn index_backfill_resumes_after_a_restart() {
    const COLLECTION_ID: &str = "col-index-resumed";
    let total = 300;
    let first_batch = 100;
    let (store, db) = open_with_collection("Resumed", "resumed-v1", COLLECTION_ID).await;
    let mutator = AutoCommitMutator::new(db.clone());
    for i in 0..total {
        create_user(&mutator, "Resumed", &format!("user-{i}")).await;
    }
    drop(mutator);

    let collection = db.require_collection("Resumed").unwrap();
    let mut schema = collection.schema().clone();
    let (fields, kind) = by_name();
    let index = {
        let txn = db.new_txn(false).await.unwrap();
        let index = {
            let datastore = txn.datastore().unwrap();
            let systemstore = txn.systemstore().unwrap();
            let mut manager =
                IndexManager::from_collection(schema.resolved_root_id(), &schema).unwrap();
            let index = manager
                .create_index_of_kind(
                    &datastore,
                    "Resumed",
                    "by_name".to_string(),
                    fields,
                    kind,
                    &schema.fields,
                )
                .await
                .unwrap();
            schema.indexes.push(index.clone());
            systemstore
                .set(
                    &CollectionKey::new(&schema.version_id).bytes(),
                    &serde_json::to_vec(&schema).unwrap(),
                )
                .await
                .unwrap();
            systemstore
                .set(
                    &CollectionNameKey::new("Resumed").bytes(),
                    schema.version_id.as_bytes(),
                )
                .await
                .unwrap();
            let lease = db
                .stage_action(
                    &systemstore,
                    COLLECTION_ID,
                    defra_core::Action::BACKFILL_INDEX,
                    &index.id.to_string(),
                )
                .await
                .unwrap();
            drop(lease);
            index
        };
        txn.commit().await.unwrap();
        index
    };
    // Every existing document sits below the fence; the id itself is spent.
    let fence = db.next_doc_short_id().await.unwrap();
    let progress_key = ActionProgressKey::new(
        COLLECTION_ID,
        defra_core::Action::BACKFILL_INDEX,
        index.id.to_string(),
    )
    .bytes();

    let indexed = Collection::new(schema.clone());
    let manager = IndexManager::from_collection(schema.resolved_root_id(), &schema).unwrap();
    let txn = db.new_txn(false).await.unwrap();
    {
        let datastore = txn.datastore().unwrap();
        let systemstore = txn.systemstore().unwrap();
        let mut source = BackfillSource::open_range(
            indexed.clone(),
            datastore.clone(),
            systemstore.clone(),
            None,
            Some(fence),
        )
        .await
        .unwrap();
        let batch = manager
            .index_batch_from(&datastore, "by_name", &mut source, &schema, first_batch)
            .await
            .unwrap();
        assert_eq!(batch.indexed, first_batch);
        assert!(!batch.exhausted);
        systemstore
            .set(
                &progress_key,
                &encode_progress(fence, batch.last_doc_short_id.unwrap()),
            )
            .await
            .unwrap();
    }
    txn.commit().await.unwrap();
    assert_eq!(index_entries(&db, "Resumed", index.id).await, first_batch);
    drop(db);

    let db = DB::open_from_arc(store).await.unwrap();
    assert_eq!(index_entries(&db, "Resumed", index.id).await, total);
    assert_eq!(entries_for(&db, "Resumed", "user-0").await, 1);
    assert_eq!(entries_for(&db, "Resumed", "user-299").await, 1);
    assert!(!db
        .list_index_actions(COLLECTION_ID)
        .await
        .unwrap()
        .contains_key(&index.id));
    assert!(!progress_recorded(&db, COLLECTION_ID, index.id).await);
    assert!(db
        .require_collection("Resumed")
        .unwrap()
        .get_indexes()
        .iter()
        .any(|existing| existing.name == "by_name"));
}

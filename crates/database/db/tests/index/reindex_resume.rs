//! A reindex a stop left part way resumes at the next open after the last
//! page it committed: the documents those pages indexed are not walked again.

use std::sync::Arc;

use db::{AutoCommitMutator, DB};
use document::{Document, NormalValue};
use query::DocMutator;
use storage::corekv::{IterOptions, Iterator as _};
use storage::keys::datastore::{IndexDataStoreKey, IndexedField};
use storage::RegolithStore;

const AGES: [i64; 4] = [10, 20, 30, 40];

/// Documents the committed pages walked before the stop.
const WALKED: usize = 2;

/// What the committed pages indexed each walked document at, distinct from its
/// stored value, as a migration's lens would make it. A walk that goes over the
/// document again indexes the stored value instead.
const PAGE_OFFSET: i64 = 100;

struct Users {
    short_id: u32,
    index_id: u32,
    doc_short_ids: Vec<u64>,
}

impl Users {
    fn entry(&self, doc: usize, age: i64) -> Vec<u8> {
        IndexDataStoreKey::with_doc_short_id(
            self.short_id,
            self.index_id,
            vec![IndexedField::ascending(NormalValue::Int(age))],
            self.doc_short_ids[doc],
        )
        .try_bytes()
        .unwrap()
    }

    fn prefix(&self) -> Vec<u8> {
        IndexDataStoreKey::index_prefix(self.short_id, self.index_id)
    }
}

async fn add_users(db: &Arc<DB<RegolithStore>>) -> Users {
    db.add_collection(
        &defra_core::RequestContext::ambient(),
        "type Users { name: String age: Int @index }",
    )
    .await
    .unwrap();
    let collection = db.require_collection("Users").unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    let mut doc_ids = Vec::new();
    for (user, age) in AGES.iter().enumerate() {
        let mut doc = Document::new(collection.schema().clone());
        doc.set("name", NormalValue::String(format!("u{user}")))
            .unwrap();
        doc.set("age", NormalValue::Int(*age)).unwrap();
        doc_ids.push(mutator.create("Users", doc).await.unwrap().doc_id);
    }
    let short_id = collection.resolved_root_id();
    let txn = db.new_txn(true).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    let mut doc_short_ids = Vec::new();
    for doc_id in &doc_ids {
        doc_short_ids.push(
            db::docid::map::get_doc_short_id(&systemstore, short_id, &doc_id.to_string())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    drop(systemstore);
    txn.discard().unwrap();
    Users {
        short_id,
        index_id: collection.schema().indexes[0].id,
        doc_short_ids,
    }
}

async fn index_keys(db: &DB<RegolithStore>, prefix: Vec<u8>) -> Vec<Vec<u8>> {
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iter = datastore
        .iterator(IterOptions::new().with_prefix(prefix).with_keys_only(true))
        .await
        .unwrap();
    let mut keys = Vec::new();
    while let Some(pair) = iter.next().await.unwrap() {
        keys.push(pair.key.to_vec());
    }
    iter.close().await.unwrap();
    drop(iter);
    drop(datastore);
    txn.discard().unwrap();
    keys.sort();
    keys
}

/// Leaves the store as a rebuild that stopped after its walk committed the
/// first `WALKED` documents does: the clear removed every entry, the committed
/// pages put back the walked documents' entries, and the progress names the
/// last of them.
async fn stop_after_walked(db: &DB<RegolithStore>, users: &Users) {
    let txn = db.new_txn(false).await.unwrap();
    let datastore = txn.datastore().unwrap();
    for (doc, age) in AGES.iter().enumerate() {
        datastore.delete(&users.entry(doc, *age)).await.unwrap();
    }
    for (doc, age) in AGES.iter().enumerate().take(WALKED) {
        datastore
            .set(&users.entry(doc, age + PAGE_OFFSET), &[])
            .await
            .unwrap();
    }
    let collection_id = db
        .require_collection("Users")
        .unwrap()
        .collection_id()
        .to_owned();
    let owed = serde_json::json!({ "token": 7, "migration_destination": null });
    let progress = serde_json::json!({
        "token": 7,
        "stage": { "Walk": { "fence": null, "last": users.doc_short_ids[WALKED - 1] } },
        "deferred": [],
    });
    let systemstore = txn.systemstore().unwrap();
    for (prefix, value) in [("/reindex/owed/", owed), ("/reindex/progress/", progress)] {
        systemstore
            .set(
                format!("{prefix}{collection_id}").as_bytes(),
                &serde_json::to_vec(&value).unwrap(),
            )
            .await
            .unwrap();
    }
    drop((datastore, systemstore));
    txn.commit().await.unwrap();
}

/// The open settles the owed reindex from the committed cursor: the walked
/// documents keep the entries their pages wrote, and only the documents after
/// the cursor are indexed again. A walk that started over would clear the
/// walked documents' entries and index their stored values.
#[test]
#[mantra_macros::req_test("defra.index.owed_reindex.resume")]
fn the_open_resumes_a_stopped_walk_after_its_last_committed_page() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let db = Arc::new(DB::open(store.clone()).await.unwrap());
        let users = add_users(&db).await;
        let stored: Vec<_> = AGES
            .iter()
            .enumerate()
            .map(|(doc, age)| users.entry(doc, *age))
            .collect();
        let mut sorted = stored.clone();
        sorted.sort();
        assert_eq!(index_keys(&db, users.prefix()).await, sorted);

        stop_after_walked(&db, &users).await;
        assert_eq!(db.owed_reindexes().await.unwrap().len(), 1);
        drop(db);

        let db = DB::open(store).await.unwrap();
        assert!(db.owed_reindexes().await.unwrap().is_empty());
        let mut expected: Vec<_> = AGES
            .iter()
            .enumerate()
            .map(|(doc, age)| {
                if doc < WALKED {
                    users.entry(doc, age + PAGE_OFFSET)
                } else {
                    users.entry(doc, *age)
                }
            })
            .collect();
        expected.sort();
        assert_eq!(index_keys(&db, users.prefix()).await, expected);
    })
    .unwrap()
}

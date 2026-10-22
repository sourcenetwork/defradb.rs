//! Documents written before part IDs existed are brought into the field
//! format at open, a page at a time, and read exactly as before.
//!
//! A legacy store is made from a current one by doing what an earlier release
//! did: no part IDs for the collection version and each document one whole
//! CBOR value. (The fixtures main wrote are opened end to end in
//! `crates/embedded/tests/legacy_store_rewrite.rs`.)

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use db::definition::migration::document_parts::{CURRENT_DOCUMENT_FORMAT, DOCUMENT_FORMAT_KEY};
use db::{AutoCommitMutator, DB};
use document::stored::StoredDocument;
use document::{DocID, Document, NormalValue};
use query::mutator::DocMutator;
use storage::corekv::{
    private::Sealed, IterOptions, Reader, Result, RetryAttempt, RetryAttemptError,
    RetryAttemptValue, RetryOptions, Store, Txn, Writer,
};
use storage::RegolithStore;

const SDL: &str = r#"
type Users {
    name: String
    age: Int
    bio: String
    tags: [String]
}
"#;

/// Enough documents for several pages of the migration (a page is 1 MiB).
const DOCUMENTS: usize = 240;

type Reads = BTreeMap<String, BTreeMap<String, NormalValue>>;

fn reads_of(doc: &Document) -> BTreeMap<String, NormalValue> {
    doc.values()
        .keys()
        .filter_map(|name| {
            doc.get(name)
                .filter(|value| !value.is_nil())
                .map(|value| (name.clone(), value.clone()))
        })
        .collect()
}

async fn read_all(db: &Arc<DB<RegolithStore>>, ids: &[DocID]) -> Reads {
    let mutator = AutoCommitMutator::new(db.clone());
    let mut reads = Reads::new();
    for id in ids {
        let doc = mutator.get_for_update("Users", id).await.unwrap().unwrap();
        reads.insert(id.to_string(), reads_of(&doc));
    }
    reads
}

/// Raw entries under one prefix of the whole store.
async fn raw(store: &RegolithStore, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let txn = store.new_txn(true).await.unwrap();
    let mut iter = txn
        .iterator(IterOptions::new().with_prefix(prefix.to_vec()))
        .await
        .unwrap();
    let mut entries = Vec::new();
    while let Some(pair) = iter.next().await.unwrap() {
        entries.push((pair.key.to_vec(), pair.value.to_vec()));
    }
    entries
}

/// Writes documents with this release, then turns the store into what an
/// earlier release left: no part IDs, whole CBOR values, no format stamp.
/// Returns the documents' IDs and how they read.
async fn legacy_store(path: &Path) -> (Vec<DocID>, Reads) {
    let store = RegolithStore::open(path).unwrap();
    let db = Arc::new(DB::open(store.clone()).await.unwrap());
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    let users = db.get_collection("Users").unwrap().unwrap().schema_arc();
    let mut ids = Vec::new();
    for i in 0..DOCUMENTS {
        let doc = Document::from_json_str(
            &format!(
                r#"{{"name": "user {i}", "age": {i}, "bio": "{}", "tags": ["t{i}", "x"]}}"#,
                "b".repeat(8 << 10)
            ),
            users.clone(),
        )
        .unwrap();
        ids.push(mutator.create("Users", doc).await.unwrap().doc_id);
    }
    let reads = read_all(&db, &ids).await;
    // Dropped, not closed: closing the database closes the store under it.
    drop(db);

    let mut txn = store.new_txn(false).await.unwrap();
    for (key, _) in raw(&store, b"s/parts/").await {
        txn.delete(&key).await.unwrap();
    }
    for (key, value) in raw(&store, b"d/d/").await {
        let doc = Document::from_storage_cbor(&value, users.clone()).unwrap();
        txn.set(&key, &doc.to_cbor().unwrap()).await.unwrap();
    }
    txn.delete(&[b"s", DOCUMENT_FORMAT_KEY].concat())
        .await
        .unwrap();
    txn.commit().await.unwrap();
    store.close().await.unwrap();
    (ids, reads)
}

async fn assert_migrated(store: &RegolithStore) {
    let documents = raw(store, b"d/d/").await;
    assert_eq!(documents.len(), DOCUMENTS);
    assert!(documents
        .iter()
        .all(|(_, value)| StoredDocument::is_field_format(value)));
    assert_eq!(raw(store, b"s/parts/").await.len(), 1);
    assert!(raw(store, b"s/system/document_format/cursor/")
        .await
        .is_empty());
    assert_eq!(
        raw(store, &[b"s", DOCUMENT_FORMAT_KEY].concat()).await,
        vec![(
            [b"s", DOCUMENT_FORMAT_KEY].concat(),
            CURRENT_DOCUMENT_FORMAT.to_be_bytes().to_vec()
        )]
    );
}

#[test]
fn documents_written_before_part_ids_get_field_operands_and_read_the_same() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let (ids, before) = legacy_store(dir.path()).await;
        let store = RegolithStore::open(dir.path()).unwrap();
        assert!(raw(&store, b"d/d/")
            .await
            .iter()
            .all(|(_, value)| !StoredDocument::is_field_format(value)));

        let db = Arc::new(DB::open(store.clone()).await.unwrap());
        assert_eq!(read_all(&db, &ids).await, before);
        drop(db);
        assert_migrated(&store).await;

        // Opening again changes nothing.
        let entries = raw(&store, b"").await;
        let db = Arc::new(DB::open(store.clone()).await.unwrap());
        assert_eq!(read_all(&db, &ids).await, before);
        drop(db);
        assert!(raw(&store, b"").await == entries);
    })
    .unwrap()
}

/// A store whose engine-retried transactions stop after `left` commits, as a
/// power cut would stop the open.
struct CutAfter {
    inner: RegolithStore,
    left: AtomicUsize,
}

impl Sealed for CutAfter {}

#[async_trait]
impl Store for CutAfter {
    async fn allocate(&self, key: &[u8], count: u64) -> storage::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: RetryOptions,
        attempt: RetryAttempt,
    ) -> std::result::Result<RetryAttemptValue, RetryAttemptError> {
        if self
            .left
            .try_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(1)
            })
            .is_err()
        {
            return Err(RetryAttemptError::Storage(storage::Error::Other(
                "power cut".into(),
            )));
        }
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A power cut before each transaction the open commits: after the part IDs,
/// after each page, before the stamp. The next open finishes the migration
/// from the cursor, and every document reads as before.
#[test]
fn a_power_cut_between_any_two_pages_resumes_and_reads_the_same() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("legacy");
        let (ids, before) = legacy_store(&original).await;
        for left in 0.. {
            let path = dir.path().join(format!("cut-{left}"));
            copy_dir(&original, &path);
            let cut = CutAfter {
                inner: RegolithStore::open(&path).unwrap(),
                left: AtomicUsize::new(left),
            };
            let finished = match DB::open(cut).await {
                Ok(db) => {
                    db.close().await.unwrap();
                    true
                }
                Err(error) => {
                    assert!(error.to_string().contains("power cut"), "{error}");
                    false
                }
            };
            let store = RegolithStore::open(&path).unwrap();
            let db = Arc::new(DB::open(store.clone()).await.unwrap());
            assert_eq!(
                read_all(&db, &ids).await,
                before,
                "cut after {left} commits"
            );
            drop(db);
            assert_migrated(&store).await;
            store.close().await.unwrap();
            if finished {
                assert!(left > 3, "the open committed only {left} transactions");
                break;
            }
        }
    })
    .unwrap()
}

/// A whole value the field format would not read back the same (here a field
/// the schema does not have) is left whole, and still reads the same.
#[test]
fn a_document_that_would_read_differently_stays_whole() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let (ids, _) = legacy_store(dir.path()).await;
        let store = RegolithStore::open(dir.path()).unwrap();
        let (key, value) = raw(&store, b"d/d/").await.remove(0);
        let scratch = DB::open(RegolithStore::in_memory().unwrap()).await.unwrap();
        scratch
            .create_collections_atomic(query::parse_sdl(SDL).unwrap())
            .await
            .unwrap();
        let users = scratch
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .schema_arc();
        let mut doc = Document::from_storage_cbor(&value, users).unwrap();
        doc.set_unchecked("retired".to_string(), NormalValue::String("kept".into()));
        let whole = doc.to_cbor().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        txn.set(&key, &whole).await.unwrap();
        txn.commit().await.unwrap();

        drop(DB::open(store.clone()).await.unwrap());
        let documents = raw(&store, b"d/d/").await;
        let (_, kept) = documents.iter().find(|(stored, _)| *stored == key).unwrap();
        assert_eq!(*kept, whole);
        assert_eq!(
            documents
                .iter()
                .filter(|(_, value)| StoredDocument::is_field_format(value))
                .count(),
            ids.len() - 1
        );
    })
    .unwrap()
}

//! Two creates of the same content derive the same DocID. Their blocks are
//! content-addressed and never conflict, so the DocID mapping read is the one
//! race that decides: exactly one of the creates commits.

use db::{AutoCommitMutator, DbDocMutator, LensedAutoCommitFetcher, DB};
use document::Document;
use query::mutator::DocMutator;
use query::runner::DocFetcher;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use std::sync::Arc;
use storage::RegolithStore;

const DOC: &str = r#"{"name": "same", "size": 3}"#;

fn collection() -> CollectionVersion {
    CollectionVersion::new(
        "Twin",
        "twin-v1",
        "col-twin",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "name", FieldKind::string()),
            FieldDescription::new("3", "size", FieldKind::int()),
        ],
    )
}

async fn open() -> Arc<DB<RegolithStore>> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).expect("create db"));
    db.create_collection(collection())
        .await
        .expect("create collection");
    db
}

#[test]
fn two_identical_creates_exactly_one_commits() {
    workers::block_on(async {
        let db = open().await;
        // Both transactions take their snapshot before either commits.
        let first = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.expect("txn one"));
        let second = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.expect("txn two"));

        let one = first
            .create("Twin", Document::from_json_str(DOC).unwrap())
            .await
            .expect("first create");
        let two = second
            .create("Twin", Document::from_json_str(DOC).unwrap())
            .await
            .expect("second create");
        assert_eq!(
            one.doc_id, two.doc_id,
            "identical content derives one DocID"
        );

        first
            .take_txn()
            .await
            .expect("take first")
            .commit()
            .await
            .expect("the first create commits");
        let error = second
            .take_txn()
            .await
            .expect("take second")
            .commit()
            .await
            .expect_err("the second identical create must not commit");
        assert!(error.is_txn_conflict(), "{error}");

        let docs = LensedAutoCommitFetcher::new(Arc::clone(&db))
            .get_all("Twin")
            .await
            .expect("read back");
        assert_eq!(docs.len(), 1);
    })
    .unwrap()
}

#[test]
fn concurrent_identical_autocommit_creates_exactly_one_commits() {
    workers::block_on(async {
        const WRITERS: usize = 16;
        let db = open().await;
        let barrier = Arc::new(regolith::sync::Barrier::new(WRITERS));
        let tasks: Vec<_> = (0..WRITERS)
            .map(|_| {
                let mutator = AutoCommitMutator::new(Arc::clone(&db));
                let barrier = Arc::clone(&barrier);
                workers::spawn(None, "test", move || async move {
                    barrier.wait().await;
                    mutator
                        .create("Twin", Document::from_json_str(DOC).unwrap())
                        .await
                })
                .unwrap()
            })
            .collect();

        let mut committed = Vec::new();
        for task in tasks {
            match task.await.expect("writer task") {
                Ok(result) => committed.push(result.doc_id),
                Err(error) => assert!(
                    error.to_string().contains("already exists"),
                    "a losing create reports the existing document: {error}"
                ),
            }
        }
        assert_eq!(committed.len(), 1, "exactly one identical create commits");

        let docs = LensedAutoCommitFetcher::new(Arc::clone(&db))
            .get_all("Twin")
            .await
            .expect("read back");
        assert_eq!(docs.len(), 1);
    })
    .unwrap()
}

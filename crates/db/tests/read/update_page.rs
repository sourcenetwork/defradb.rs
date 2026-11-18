//! What a page of the update journal reads from the store: its rows in one
//! scan, however many they are.

use crate::common::counting_store::CountingStore;
use crate::common::schema::users_schema;
use db::{AutoCommitMutator, LensedAutoCommitFetcher, DB};
use document::Document;
use query::{DocMutator, QueryExecutor, QueryRequest};
use std::sync::Arc;
use storage::RegolithStore;

const UPDATES: usize = 8;

type Db = Arc<DB<CountingStore<RegolithStore>>>;

async fn updated_document() -> Db {
    let db = Arc::new(DB::new(CountingStore::new(RegolithStore::in_memory().unwrap())).unwrap());
    db.create_collection(users_schema()).await.unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    let mut doc = Document::new(users_schema());
    doc.set("name", "initial").unwrap();
    let id = mutator.create("Users", doc).await.unwrap().doc_id;
    for update in 0..UPDATES {
        let mut doc = Document::new(users_schema());
        doc.set_id(id.clone());
        doc.set("name", format!("update-{update}")).unwrap();
        mutator
            .update("Users", doc, ["name".to_string()].into_iter().collect())
            .await
            .unwrap();
    }
    db
}

/// The point reads, scanned keys and journal scans of one page of `limit` rows.
async fn page_cost(db: &Db, limit: usize) -> (usize, usize, usize) {
    let runner = query::QueryRunner::with_provider(
        LensedAutoCommitFetcher::new(db.clone()),
        db::DbCollectionProvider::new_arc(db.clone()),
    );
    let store = db.store();
    store.take_scan_prefixes();
    let (gets, keys) = (store.point_gets(), store.keys_read());
    let response = runner
        .execute(QueryRequest::new(format!(
            r#"{{ _documentUpdates(collection: "Users", limit: {limit}) {{ next entries {{ cursor }} }} }}"#
        )))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let page = &response.data.unwrap()["_documentUpdates"];
    assert_eq!(page["next"], limit.to_string());
    let cursors: Vec<String> = page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["cursor"].as_str().unwrap().to_string())
        .collect();
    let expected: Vec<String> = (1..=limit).map(|cursor| cursor.to_string()).collect();
    assert_eq!(cursors, expected);
    let journal_scans = store
        .take_scan_prefixes()
        .iter()
        .filter(|prefix| {
            prefix
                .windows(b"/arrival/updates/".len())
                .any(|window| window == b"/arrival/updates/")
        })
        .count();
    (
        store.point_gets() - gets,
        store.keys_read() - keys,
        journal_scans,
    )
}

#[test]
fn a_page_reads_its_rows_in_one_scan() {
    workers::block_on(async {
        let db = updated_document().await;
        let (one_row_gets, one_row_keys, one_row_scans) = page_cost(&db, 1).await;
        let (page_gets, page_keys, page_scans) = page_cost(&db, UPDATES).await;
        assert_eq!(
            page_gets, one_row_gets,
            "a page makes no point read per row"
        );
        assert_eq!(page_keys - one_row_keys, UPDATES - 1);
        assert_eq!((one_row_scans, page_scans), (1, 1));
    })
    .unwrap();
}

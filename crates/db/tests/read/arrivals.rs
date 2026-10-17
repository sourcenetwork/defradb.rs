use crate::common::schema::users_schema;
use db::{AutoCommitMutator, LensedAutoCommitFetcher, DB};
use document::Document;
use query::{DocMutator, QueryExecutor, QueryRequest};
use std::sync::Arc;
use storage::RegolithStore;

#[test]
fn graphql_creation_cursor_pages_and_docid_lookup() {
    workers::block_on(async {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(users_schema()).await.unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    let mut ids = Vec::new();
    for name in ["one", "two", "three"] {
        let mut doc = Document::new();
        doc.set("name", name);
        ids.push(
            mutator
                .create("Users", doc)
                .await
                .unwrap()
                .doc_id
                .to_string(),
        );
    }
    let runner = query::QueryRunner::with_provider(
        LensedAutoCommitFetcher::new(db.clone()),
        db::DbCollectionProvider::new_arc(db),
    );
    let result = runner.execute(QueryRequest::new(r#"{ _documentArrivals(collection: "Users", after: "0", limit: 2) { head next entries { cursor docID } } }"#)).await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let page = &result.data.unwrap()["_documentArrivals"];
    assert_eq!(page["head"], "3");
    assert_eq!(page["next"], "2");
    assert_eq!(page["entries"][0]["docID"], ids[0]);
    assert_eq!(page["entries"][1]["docID"], ids[1]);
    let result = runner.execute(QueryRequest::new(format!(r#"{{ _documentArrivals(collection: "Users", after: "0", docID: ["{}", "{}"]) {{ head next entries {{ cursor docID }} }} }}"#, ids[2], ids[0]))).await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let page = &result.data.unwrap()["_documentArrivals"];
    assert_eq!(page["entries"][0]["cursor"], "1");
    assert_eq!(page["entries"][1]["cursor"], "3");
})
    .unwrap()
}

#[test]
fn hidden_arrivals_advance_page_without_disclosing_document_ids() {
    workers::block_on(async {
        use acp::DocumentACP;
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let mut schema = users_schema();
        schema.policy = Some(schema::PolicyDescription::new("arrival-policy", "users"));
        schema.is_branchable = false;
        db.create_collection(schema).await.unwrap();
        let mut doc = Document::new();
        doc.set("name", "secret");
        let id = AutoCommitMutator::new(db.clone())
            .create("Users", doc)
            .await
            .unwrap()
            .doc_id
            .to_string();
        let acp = Arc::new(acp::LocalDocumentACP::new(Arc::new(
            acp::MemoryAcpStore::new(),
        )));
        let owner =
            identity::Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
        acp.register_doc_object(&owner, "arrival-policy", "users", &id)
            .await
            .unwrap();
        let runner = query::QueryRunner::with_provider(
            LensedAutoCommitFetcher::new(db.clone()),
            db::DbCollectionProvider::new_arc(db),
        )
        .with_acp(acp);
        let request =
            r#"{ _documentArrivals(collection:"Users") { head next entries { cursor docID } } }"#;
        let response = runner.execute(QueryRequest::new(request)).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let page = &response.data.unwrap()["_documentArrivals"];
        assert_eq!(page["next"], "1");
        assert_eq!(page["entries"], serde_json::json!([]));
        let response = runner
            .execute(QueryRequest::new(request).with_identity(Some(owner)))
            .await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(
            response.data.unwrap()["_documentArrivals"]["entries"][0]["docID"],
            id
        );
    })
    .unwrap()
}

#[test]
fn explicit_transaction_arrival_is_invisible_and_rollback_discards_it() {
    workers::block_on(async {
        use query::fetcher::{DocFetcher, DocumentArrivalOptions};
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema()).await.unwrap();
        let mutator = db::DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
        let mut doc = Document::new();
        doc.set("name", "pending");
        let id = mutator.create("Users", doc).await.unwrap().doc_id;
        let reader = db::DbDocFetcher::new(mutator.take_txn().await.unwrap());
        let options = DocumentArrivalOptions {
            collection: "Users".into(),
            after: 0,
            limit: 10,
            doc_ids: None,
        };
        let page = reader.get_document_arrivals(&options).await.unwrap();
        assert_eq!(page.head, 0);
        assert!(page.entries.is_empty());
        reader.take_txn().await.unwrap().discard().unwrap();
        let reader = LensedAutoCommitFetcher::new(db);
        assert_eq!(
            reader.get_document_arrivals(&options).await.unwrap().head,
            0
        );
        assert!(reader
            .get_by_ids("Users", &[id.to_string()])
            .await
            .unwrap()
            .docs()
            .is_empty());
    })
    .unwrap()
}

#[test]
fn concurrent_autocommit_arrivals_are_contiguous() {
    workers::block_on(async {
        use query::fetcher::{DocFetcher, DocumentArrivalOptions};
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema()).await.unwrap();
        let mut creates: futures::stream::FuturesUnordered<_> = (0..64)
            .map(|index| {
                let db = db.clone();
                workers::spawn(None, "test", move || async move {
                    let mut doc = Document::new();
                    doc.set("name", format!("arrival-{index}"));
                    AutoCommitMutator::new(db).create("Users", doc).await
                })
                .unwrap()
            })
            .collect();
        let mut ids = rapidhash::RapidHashSet::default();
        while let Some(created) = futures::StreamExt::next(&mut creates).await {
            ids.insert(created.unwrap().unwrap().doc_id.to_string());
        }
        let page = LensedAutoCommitFetcher::new(db)
            .get_document_arrivals(&DocumentArrivalOptions {
                collection: "Users".into(),
                after: 0,
                limit: 64,
                doc_ids: None,
            })
            .await
            .unwrap();
        assert_eq!(page.head, 64);
        assert_eq!(page.next, 64);
        assert_eq!(page.entries.len(), 64);
        for (index, entry) in page.entries.iter().enumerate() {
            assert_eq!(entry.cursor, index as u64 + 1);
            assert!(ids.remove(&entry.doc_id));
        }
        assert!(ids.is_empty());
    })
    .unwrap()
}

#[test]
fn overlapping_explicit_transaction_creates_both_commit() {
    workers::block_on(async {
        use query::fetcher::{DocFetcher, DocumentArrivalOptions};
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema()).await.unwrap();
        let first = db::DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
        let second = db::DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
        let mut one = Document::new();
        one.set("name", "first");
        let mut two = Document::new();
        two.set("name", "second");
        let first_id = first.create("Users", one).await.unwrap().doc_id.to_string();
        let second_id = second
            .create("Users", two)
            .await
            .unwrap()
            .doc_id
            .to_string();
        second.take_txn().await.unwrap().commit().await.unwrap();
        first.take_txn().await.unwrap().commit().await.unwrap();
        let reader = LensedAutoCommitFetcher::new(db);
        let page = reader
            .get_document_arrivals(&DocumentArrivalOptions {
                collection: "Users".into(),
                after: 0,
                limit: 10,
                doc_ids: None,
            })
            .await
            .unwrap();
        assert_eq!(page.head, 2);
        assert_eq!(page.entries[0].doc_id, second_id);
        assert_eq!(page.entries[1].doc_id, first_id);
    })
    .unwrap()
}

#[test]
fn explicit_transaction_arrival_gets_a_cursor_only_on_commit() {
    workers::block_on(async {
        use query::fetcher::{DocFetcher, DocumentArrivalOptions};
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        db.create_collection(users_schema()).await.unwrap();
        let mutator = db::DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
        let mut doc = Document::new();
        doc.set("name", "pending");
        let id = mutator
            .create("Users", doc)
            .await
            .unwrap()
            .doc_id
            .to_string();
        let reader = db::DbDocFetcher::new(mutator.take_txn().await.unwrap());
        let options = DocumentArrivalOptions {
            collection: "Users".into(),
            after: 0,
            limit: 10,
            doc_ids: Some(vec![id.clone()]),
        };
        assert!(reader
            .get_document_arrivals(&options)
            .await
            .unwrap()
            .entries
            .is_empty());
        reader.take_txn().await.unwrap().commit().await.unwrap();
        let reader = LensedAutoCommitFetcher::new(db);
        let page = reader.get_document_arrivals(&options).await.unwrap();
        assert_eq!(page.entries[0].cursor, 1);
        assert_eq!(page.entries[0].doc_id, id);
    })
    .unwrap()
}

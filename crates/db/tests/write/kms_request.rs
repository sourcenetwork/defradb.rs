use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use async_trait::async_trait;
use db::AutoCommitMutator;
use document::{DocID, Document};
use query::{
    mutator::{CreateResult, DeleteResult, DocMutator, UpdateResult},
    QueryExecutor, QueryRequest,
};
use rapidhash::RapidHashSet;
use storage::RegolithStore;

struct ConflictOnce {
    inner: AutoCommitMutator<RegolithStore>,
    first: AtomicBool,
}

#[async_trait]
impl DocMutator for ConflictOnce {
    fn requires_write_preparation(&self) -> bool {
        true
    }

    async fn prepare_write(
        &self,
        collection: &str,
        doc: Document,
        fields: Option<RapidHashSet<String>>,
        config: Option<defra_core::encryption::EncryptionConfig>,
    ) -> query::error::Result<Document> {
        self.inner
            .prepare_write(collection, doc, fields, config)
            .await
    }

    async fn create(&self, collection: &str, doc: Document) -> query::error::Result<CreateResult> {
        if self.first.swap(false, Ordering::SeqCst) {
            return Err(query::error::QueryError::transaction_conflict(
                "injected conflict",
            ));
        }
        self.inner.create(collection, doc).await
    }

    async fn update(
        &self,
        collection: &str,
        doc: Document,
        fields: RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        if self.first.swap(false, Ordering::SeqCst) {
            return Err(query::error::QueryError::transaction_conflict(
                "injected conflict",
            ));
        }
        self.inner.update(collection, doc, fields).await
    }

    async fn delete(&self, collection: &str, id: &DocID) -> query::error::Result<DeleteResult> {
        self.inner.delete(collection, id).await
    }

    async fn exists(&self, collection: &str, id: &DocID) -> query::error::Result<bool> {
        self.inner.exists(collection, id).await
    }

    async fn get_for_update(
        &self,
        collection: &str,
        id: &DocID,
    ) -> query::error::Result<Option<Document>> {
        self.inner.get_for_update(collection, id).await
    }
}

#[tokio::test]
async fn retried_request_reuses_prepared_kms_keys() {
    let fixture = super::kms::fixture().await;
    let runner = query::QueryRunner::with_provider(
        db::LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
        db::DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
    )
    .with_mutator(Arc::new(ConflictOnce {
        inner: AutoCommitMutator::new(Arc::clone(&fixture.db)),
        first: AtomicBool::new(true),
    }));
    let request = QueryRequest::new(
        r#"mutation { add_Secrets(input: {left: "one", right: "two"}, encryptFields: [left, right]) { _docID } }"#,
    );
    let prepared = runner.prepare_request(&request).await.unwrap();
    assert_eq!(fixture.kms.0.load(Ordering::SeqCst), 2);
    let first = runner
        .execute_prepared(request.clone(), prepared.clone())
        .await;
    assert!(first.is_transaction_conflict(), "{:?}", first.errors);
    let second = runner.execute_prepared(request, prepared).await;
    assert!(!second.has_errors(), "{:?}", second.errors);
    assert_eq!(fixture.kms.0.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn retried_update_reuses_prepared_kms_keys() {
    let fixture = super::kms::fixture().await;
    let inner = AutoCommitMutator::new(Arc::clone(&fixture.db));
    let created = inner
        .create(
            "Secrets",
            Document::from_json_str(r#"{"left":"one","right":"two"}"#).unwrap(),
        )
        .await
        .unwrap();
    let runner = query::QueryRunner::with_provider(
        db::LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
        db::DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
    )
    .with_mutator(Arc::new(ConflictOnce {
        inner,
        first: AtomicBool::new(true),
    }));
    let request = QueryRequest::new(format!(
        r#"mutation {{ update_Secrets(docID: "{}", input: {{left: "three", right: "four"}}, encryptFields: [left, right]) {{ _docID }} }}"#,
        created.doc_id
    ));
    let prepared = runner.prepare_request(&request).await.unwrap();
    assert_eq!(fixture.kms.0.load(Ordering::SeqCst), 2);
    let first = runner
        .execute_prepared(request.clone(), prepared.clone())
        .await;
    assert!(first.is_transaction_conflict(), "{:?}", first.errors);
    let second = runner.execute_prepared(request, prepared).await;
    assert!(!second.has_errors(), "{:?}", second.errors);
    assert_eq!(fixture.kms.0.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn prepared_request_updates_a_document_created_by_an_earlier_mutation() {
    let fixture = super::kms::fixture().await;
    let runner = query::QueryRunner::with_provider(
        db::LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
        db::DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&fixture.db))));
    let request = QueryRequest::new(
        r#"mutation {
        first: add_Secrets(input: {left: "one", right: "two"}, encryptFields: [left, right]) { _docID }
        second: update_Secrets(filter: {left: {_eq: "one"}}, input: {right: "three"}, encryptFields: [right]) { right }
    }"#,
    );
    let prepared = runner.prepare_request(&request).await.unwrap();
    let response = runner.execute_prepared(request, prepared).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    assert_eq!(response.data.unwrap()["second"][0]["right"], "three");
    assert_eq!(fixture.kms.0.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn prepared_request_inherits_encryption_from_an_earlier_create() {
    let fixture = super::kms::fixture().await;
    let runner = query::QueryRunner::with_provider(
        db::LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
        db::DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&fixture.db))));
    let request = QueryRequest::new(
        r#"mutation {
        first: add_Secrets(input: {left: "one"}, encrypt: true) { _docID }
        second: update_Secrets(filter: {left: {_eq: "one"}}, input: {right: "two"}) { right }
    }"#,
    );
    let prepared = runner.prepare_request(&request).await.unwrap();
    assert_eq!(fixture.kms.0.load(Ordering::SeqCst), 2);
    let response = runner.execute_prepared(request, prepared).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    assert_eq!(response.data.unwrap()["second"][0]["right"], "two");
    assert_eq!(fixture.kms.0.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn replayed_prepared_create_does_not_overwrite_a_committed_document() {
    let fixture = super::kms::fixture().await;
    let runner = query::QueryRunner::with_provider(
        db::LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
        db::DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&fixture.db))));
    let request = QueryRequest::new(
        r#"mutation { add_Secrets(input: {left: "one"}, encryptFields: [left]) { _docID } }"#,
    );
    let prepared = runner.prepare_request(&request).await.unwrap();
    let first = runner
        .execute_prepared(request.clone(), prepared.clone())
        .await;
    assert!(!first.has_errors(), "{:?}", first.errors);
    let id = first.data.unwrap()["add_Secrets"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();
    let second = runner.execute_prepared(request, prepared).await;
    assert!(second.has_errors());
    let response = runner
        .execute(QueryRequest::new("{ Secrets { _docID left } }"))
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let docs = response.data.unwrap()["Secrets"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0]["_docID"], id);
    assert_eq!(docs[0]["left"], "one");
    assert_eq!(fixture.kms.0.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn prepared_upsert_uses_the_state_of_preceding_mutations() {
    let fixture = super::kms::fixture().await;
    let runner = query::QueryRunner::with_provider(
        db::LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
        db::DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&fixture.db))));
    let request = QueryRequest::new(
        r#"mutation {
        first: add_Secrets(input: {left: "one"}) { _docID }
        second: upsert_Secrets(filter: {left: {_eq: "one"}}, add: {left: "unused"}, update: {right: "two"}, encryptFields: [right]) { right }
        third: delete_Secrets(filter: {right: {_eq: "two"}}) { _docID }
        fourth: upsert_Secrets(filter: {left: {_eq: "one"}}, add: {left: "replacement"}, update: {right: "unused"}, encryptFields: [left]) { left }
    }"#,
    );
    let prepared = runner.prepare_request(&request).await.unwrap();
    let response = runner.execute_prepared(request, prepared).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let data = response.data.unwrap();
    assert_eq!(data["second"][0]["right"], "two");
    assert_eq!(data["fourth"][0]["left"], "replacement");
    assert_eq!(fixture.kms.0.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn prepared_delete_and_upsert_recheck_their_filters() {
    for mutation in [
        r#"mutation { delete_Secrets(filter: {left: {_eq: "one"}}) { _docID } }"#,
        r#"mutation { upsert_Secrets(filter: {left: {_eq: "one"}}, add: {left: "new"}, update: {right: "changed"}) { _docID } }"#,
    ] {
        let fixture = super::kms::fixture().await;
        let mutator = AutoCommitMutator::new(Arc::clone(&fixture.db));
        let created = mutator
            .create(
                "Secrets",
                Document::from_json_str(r#"{"left":"one"}"#).unwrap(),
            )
            .await
            .unwrap();
        let runner = query::QueryRunner::with_provider(
            db::LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
            db::DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
        )
        .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&fixture.db))));
        let request = QueryRequest::new(mutation);
        let prepared = runner.prepare_request(&request).await.unwrap();
        let mut doc = created.document;
        doc.set("left", "two");
        mutator
            .update("Secrets", doc, ["left".to_string()].into_iter().collect())
            .await
            .unwrap();
        let response = runner.execute_prepared(request, prepared).await;
        if mutation.contains("upsert") {
            assert!(response.has_errors());
            assert!(!response.is_transaction_conflict());
        } else {
            assert!(!response.has_errors(), "{:?}", response.errors);
            assert_eq!(
                response.data.unwrap()["delete_Secrets"],
                serde_json::json!([])
            );
        }
        let doc = mutator
            .get_for_update("Secrets", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            doc.get("left"),
            Some(&document::NormalValue::String("two".into()))
        );
        assert!(doc
            .get("right")
            .is_none_or(|value| matches!(value, document::NormalValue::Null)));
    }
}

#[tokio::test]
async fn preparation_does_not_project_an_update_skipped_by_its_filter() {
    let fixture = super::kms::fixture().await;
    let mutator = AutoCommitMutator::new(Arc::clone(&fixture.db));
    let created = mutator
        .create(
            "Secrets",
            Document::from_json_str(r#"{"left":"one"}"#).unwrap(),
        )
        .await
        .unwrap();
    let runner = query::QueryRunner::with_provider(
        db::LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
        db::DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
    )
    .with_mutator(Arc::new(mutator));
    let request = QueryRequest::new(format!(
        r#"mutation {{
        skipped: update_Secrets(docID: "{}", filter: {{left: {{_eq: "absent"}}}}, input: {{left: "two"}}) {{ _docID }}
        next: upsert_Secrets(filter: {{left: {{_eq: "one"}}}}, add: {{left: "unused"}}, update: {{right: "matched"}}, encryptFields: [right]) {{ right }}
    }}"#,
        created.doc_id
    ));
    let prepared = runner.prepare_request(&request).await.unwrap();
    let response = runner.execute_prepared(request, prepared).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let data = response.data.unwrap();
    assert_eq!(data["skipped"], serde_json::json!([]));
    assert_eq!(data["next"][0]["right"], "matched");
    assert_eq!(fixture.kms.0.load(Ordering::SeqCst), 1);
}

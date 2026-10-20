use crate::common::http::HttpServer;
use db::{
    AutoCommitMutator, DbCollectionProvider, DbDocMutator, DbTransactionRegistry,
    LensedAutoCommitFetcher, DB,
};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest};
use schema::{CollectionVersion, FieldDescription, FieldKind, VectorEmbeddingDescription};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use storage::RegolithStore;

struct Fixture {
    db: Arc<DB<RegolithStore>>,
    /// Embedding requests the server has answered.
    calls: Arc<AtomicUsize>,
    _server: HttpServer,
}

impl Fixture {
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

async fn fixture() -> Fixture {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let server = HttpServer::start(&["/embeddings"], move |request| {
        let request = request.body;
        counted.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request["model"], "test-model");
        let vector = match request["input"].as_str().unwrap() {
            "first\n" => vec![1.0, 2.0],
            "second\n" => vec![3.0, 4.0],
            text => panic!("unexpected embedding input: {text}"),
        };
        json!({"data": [{"embedding": vector}]})
    });
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let mut collection = CollectionVersion::new(
        "Texts",
        "texts-v1",
        "col-texts",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "text", FieldKind::string()),
            FieldDescription::new("3", "vector", FieldKind::float64_array()),
        ],
    );
    collection
        .vector_embeddings
        .push(VectorEmbeddingDescription {
            field_name: "vector".to_string(),
            fields: vec!["text".to_string()],
            model: "test-model".to_string(),
            provider: "openai".to_string(),
            template: String::new(),
            url: server.url(),
        });
    db.create_collection(collection).await.unwrap();
    Fixture {
        db,
        calls,
        _server: server,
    }
}

fn document(text: &str) -> Document {
    let mut doc = Document::new();
    doc.set("text", text);
    doc
}

fn assert_vector(doc: &Document, values: Vec<f64>) {
    assert_eq!(doc.get("vector"), Some(&NormalValue::Float64Array(values)));
}

#[test]
fn autocommit_create_generates_embedding() {
    workers::block_on(async {
        let fixture = fixture().await;
        let mutator = AutoCommitMutator::new(Arc::clone(&fixture.db));
        let created = mutator.create("Texts", document("first")).await.unwrap();
        assert_vector(&created.document, vec![1.0, 2.0]);
        let stored = mutator
            .get_for_update("Texts", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_vector(&stored, vec![1.0, 2.0]);
    })
    .unwrap()
}

/// An explicit-transaction create never calls the embedding service inside
/// the transaction: it ends asking for the input, which is fetched outside and
/// read from the memo when the create runs again.
#[test]
fn explicit_txn_create_generates_embedding() {
    workers::block_on(async {
        let fixture = fixture().await;
        let txn = fixture.db.new_txn(false).await.unwrap();
        let mutator = DbDocMutator::new(Arc::clone(&fixture.db), txn);
        let error = mutator
            .create("Texts", document("first"))
            .await
            .unwrap_err();
        let query::error::QueryError::NeedsInput(request) = error else {
            panic!("expected an input request, got {error:?}");
        };
        assert_eq!(fixture.calls(), 0, "the create called the service");

        let inputs = query::prepared::PreparedInputMemo::default();
        inputs.insert(
            request.clone(),
            mutator.resolve_prepared_input(&request).await.unwrap(),
        );
        let created = mutator
            .with_prepared_inputs(inputs)
            .unwrap()
            .create("Texts", document("first"))
            .await
            .unwrap();
        assert_eq!(fixture.calls(), 1);
        assert_vector(&created.document, vec![1.0, 2.0]);
        mutator.take_txn().await.unwrap().commit().await.unwrap();
        let stored = AutoCommitMutator::new(Arc::clone(&fixture.db))
            .get_for_update("Texts", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_vector(&stored, vec![1.0, 2.0]);
    })
    .unwrap()
}

/// An explicit-transaction update never calls the embedding service inside
/// the transaction: it ends asking for the input, which is fetched outside and
/// read from the memo when the update runs again.
#[test]
fn explicit_txn_update_regenerates_embedding() {
    workers::block_on(async {
        let fixture = fixture().await;
        let autocommit = AutoCommitMutator::new(Arc::clone(&fixture.db));
        let created = autocommit.create("Texts", document("first")).await.unwrap();
        let txn = fixture.db.new_txn(false).await.unwrap();
        let mutator = DbDocMutator::new(Arc::clone(&fixture.db), txn);
        let mut doc = mutator
            .get_for_update("Texts", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        doc.set("text", "second");
        let calls_before = fixture.calls();
        let fields: rapidhash::RapidHashSet<String> = ["text".to_string()].into_iter().collect();
        let error = mutator
            .update("Texts", doc.clone(), fields.clone())
            .await
            .unwrap_err();
        let query::error::QueryError::NeedsInput(request) = error else {
            panic!("expected an input request, got {error:?}");
        };
        assert_eq!(
            fixture.calls(),
            calls_before,
            "the update called the service"
        );

        let inputs = query::prepared::PreparedInputMemo::default();
        inputs.insert(
            request.clone(),
            mutator.resolve_prepared_input(&request).await.unwrap(),
        );
        assert_eq!(fixture.calls(), calls_before + 1);
        let updated = mutator
            .with_prepared_inputs(inputs)
            .unwrap()
            .update("Texts", doc, fields)
            .await
            .unwrap();
        assert_eq!(fixture.calls(), calls_before + 1);
        assert_vector(&updated.document, vec![3.0, 4.0]);
        mutator.take_txn().await.unwrap().commit().await.unwrap();
        let stored = autocommit
            .get_for_update("Texts", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_vector(&stored, vec![3.0, 4.0]);
    })
    .unwrap()
}

/// A GraphQL update in an explicit transaction gets its embedding: the
/// statement fetches it between two runs, never inside one.
#[test]
fn explicit_txn_graphql_update_regenerates_embedding() {
    workers::block_on(async {
    let fixture = fixture().await;
    let created = AutoCommitMutator::new(Arc::clone(&fixture.db))
        .create("Texts", document("first"))
        .await
        .unwrap();
    let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&fixture.db)));
    let queries = query::QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
        DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
        registry,
    );
    let txn = queries.begin_txn(false).await.unwrap();
    let calls_before = fixture.calls();
    let response = queries
        .execute_in_txn(
            QueryRequest::new(format!(
                r#"mutation {{ update_Texts(docID: "{}", input: {{text: "second"}}) {{ vector }} }}"#,
                created.doc_id
            )),
            &txn,
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    assert_eq!(fixture.calls(), calls_before + 1);
    queries.commit_txn(&txn).await.unwrap();
    let stored = AutoCommitMutator::new(Arc::clone(&fixture.db))
        .get_for_update("Texts", &created.doc_id)
        .await
        .unwrap()
        .unwrap();
    assert_vector(&stored, vec![3.0, 4.0]);
})
    .unwrap()
}

/// A GraphQL create in an explicit transaction gets its embedding: the
/// statement fetches it between two runs, never inside one.
#[test]
fn explicit_txn_graphql_create_generates_embedding() {
    workers::block_on(async {
        let fixture = fixture().await;
        let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&fixture.db)));
        let queries = query::QueryRunner::with_arc_registry_and_provider(
            LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
            DbCollectionProvider::new_arc(Arc::clone(&fixture.db)),
            registry,
        );
        let txn = queries.begin_txn(false).await.unwrap();
        let response = queries
            .execute_in_txn(
                QueryRequest::new(
                    r#"mutation { create_Texts(input: {text: "first"}) { _docID vector } }"#,
                ),
                &txn,
            )
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        assert_eq!(fixture.calls(), 1);
        queries.commit_txn(&txn).await.unwrap();

        let stored = query::DocFetcher::get_all(
            &LensedAutoCommitFetcher::new(Arc::clone(&fixture.db)),
            "Texts",
        )
        .await
        .unwrap();
        assert_eq!(stored.len(), 1);
        assert_vector(&stored[0], vec![1.0, 2.0]);
    })
    .unwrap()
}

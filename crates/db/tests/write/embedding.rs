use axum::{routing::post, Json, Router};
use db::{AutoCommitMutator, DbDocMutator, DB};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use schema::{CollectionVersion, FieldDescription, FieldKind, VectorEmbeddingDescription};
use serde_json::{json, Value};
use std::sync::Arc;
use storage::RegolithStore;

struct Fixture {
    db: Arc<DB<RegolithStore>>,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn fixture() -> Fixture {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/embeddings",
                post(|Json(request): Json<Value>| async move {
                    assert_eq!(request["model"], "test-model");
                    let vector = match request["input"].as_str().unwrap() {
                        "first\n" => vec![1.0, 2.0],
                        "second\n" => vec![3.0, 4.0],
                        text => panic!("unexpected embedding input: {text}"),
                    };
                    Json(json!({"data": [{"embedding": vector}]}))
                }),
            ),
        )
        .await
        .unwrap();
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
            url: format!("http://{address}"),
        });
    db.create_collection(collection).await.unwrap();
    Fixture { db, server }
}

fn document(text: &str) -> Document {
    let mut doc = Document::new();
    doc.set("text", text);
    doc
}

fn assert_vector(doc: &Document, values: Vec<f64>) {
    assert_eq!(doc.get("vector"), Some(&NormalValue::Float64Array(values)));
}

#[tokio::test]
async fn autocommit_create_generates_embedding() {
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
}

#[tokio::test]
async fn explicit_txn_create_generates_embedding() {
    let fixture = fixture().await;
    let txn = fixture.db.new_txn(false).await.unwrap();
    let mutator = DbDocMutator::new(Arc::clone(&fixture.db), txn);
    let created = mutator.create("Texts", document("first")).await.unwrap();
    assert_vector(&created.document, vec![1.0, 2.0]);
    mutator.take_txn().await.unwrap().commit().await.unwrap();
    let stored = AutoCommitMutator::new(Arc::clone(&fixture.db))
        .get_for_update("Texts", &created.doc_id)
        .await
        .unwrap()
        .unwrap();
    assert_vector(&stored, vec![1.0, 2.0]);
}

#[tokio::test]
async fn explicit_txn_update_regenerates_embedding() {
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
    let updated = mutator
        .update("Texts", doc, ["text".to_string()].into_iter().collect())
        .await
        .unwrap();
    assert_vector(&updated.document, vec![3.0, 4.0]);
    mutator.take_txn().await.unwrap().commit().await.unwrap();
    let stored = autocommit
        .get_for_update("Texts", &created.doc_id)
        .await
        .unwrap()
        .unwrap();
    assert_vector(&stored, vec![3.0, 4.0]);
}

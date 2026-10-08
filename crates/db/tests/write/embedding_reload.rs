use axum::{routing::post, Json, Router};
use db::{AutoCommitMutator, DB};
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

async fn fixture_with_counter(counter: bool) -> Fixture {
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
                        "first\nold\n" | "5\nold\n" => vec![1.0, 2.0],
                        "first\nnew\n" | "7\nold\n" => vec![3.0, 4.0],
                        "second\nold\n" => vec![5.0, 6.0],
                        "second\nnew\n" => vec![7.0, 8.0],
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
            if counter {
                FieldDescription::new("2", "text", FieldKind::int())
                    .with_crdt_type(schema::CType::PnCounter)
            } else {
                FieldDescription::new("2", "text", FieldKind::string())
            },
            FieldDescription::new("3", "vector", FieldKind::float64_array()),
            FieldDescription::new("4", "suffix", FieldKind::string()),
        ],
    );
    collection
        .vector_embeddings
        .push(VectorEmbeddingDescription {
            field_name: "vector".to_string(),
            fields: vec!["text".to_string(), "suffix".to_string()],
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
    doc.set("suffix", "old");
    doc
}

fn assert_vector(doc: &Document, values: Vec<f64>) {
    assert_eq!(doc.get("vector"), Some(&NormalValue::Float64Array(values)));
}

#[tokio::test]
async fn update_regenerates_embedding_from_reloaded_document() {
    let fixture = fixture_with_counter(false).await;
    let mutator = AutoCommitMutator::new(Arc::clone(&fixture.db));
    let created = mutator.create("Texts", document("first")).await.unwrap();
    let mut stale = created.document.clone();
    stale.set("text", "second");
    let mut concurrent = created.document;
    concurrent.set("suffix", "new");
    mutator
        .update(
            "Texts",
            concurrent,
            ["suffix".to_string()].into_iter().collect(),
        )
        .await
        .unwrap();
    let updated = mutator
        .update("Texts", stale, ["text".to_string()].into_iter().collect())
        .await
        .unwrap();
    assert_eq!(
        updated.document.get("suffix").unwrap().as_str(),
        Some("new")
    );
    assert_vector(&updated.document, vec![7.0, 8.0]);
    let stored = mutator
        .get_for_update("Texts", &created.doc_id)
        .await
        .unwrap()
        .unwrap();
    assert_vector(&stored, vec![7.0, 8.0]);
}

#[tokio::test]
async fn update_embeds_the_applied_counter_value() {
    let fixture = fixture_with_counter(true).await;
    let mutator = AutoCommitMutator::new(Arc::clone(&fixture.db));
    let mut doc = Document::new();
    doc.set("text", 5_i64);
    doc.set("suffix", "old");
    let created = mutator.create("Texts", doc).await.unwrap();
    let mut incremented = created.document;
    incremented.set("text", 7_i64);
    incremented.set_counter_delta("text".to_string(), NormalValue::Int(2));
    let updated = mutator
        .update(
            "Texts",
            incremented,
            ["text".to_string()].into_iter().collect(),
        )
        .await
        .unwrap();
    assert_eq!(updated.document.get("text"), Some(&NormalValue::Int(7)));
    assert_vector(&updated.document, vec![3.0, 4.0]);
    let stored = mutator
        .get_for_update("Texts", &created.doc_id)
        .await
        .unwrap()
        .unwrap();
    assert_vector(&stored, vec![3.0, 4.0]);
}

use crate::common::http::HttpServer;
use db::{AutoCommitMutator, DB};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use schema::CollectionVersion;
use serde_json::json;
use std::sync::Arc;
use storage::RegolithStore;

struct Fixture {
    db: Arc<DB<RegolithStore>>,
    collection: CollectionVersion,
    _server: HttpServer,
}

impl Fixture {
    fn document(&self, text: &str) -> Document {
        let mut doc = Document::new(self.collection.clone());
        doc.set("text", text).unwrap();
        doc.set("suffix", "old").unwrap();
        doc
    }
}

async fn fixture_with_counter(counter: bool) -> Fixture {
    let server = HttpServer::start(&["/embeddings"], |request| {
        let request = request.body;
        assert_eq!(request["model"], "test-model");
        let vector = match request["input"].as_str().unwrap() {
            "first\nold\n" | "5\nold\n" => vec![1.0, 2.0],
            "first\nnew\n" | "7\nold\n" => vec![3.0, 4.0],
            "6\nold\n" => vec![9.0, 10.0],
            "8\nold\n" => vec![11.0, 12.0],
            "second\nold\n" => vec![5.0, 6.0],
            "second\nnew\n" => vec![7.0, 8.0],
            text => panic!("unexpected embedding input: {text}"),
        };
        json!({"data": [{"embedding": vector}]})
    });
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let text = if counter {
        r#"Int @crdt(type: "pncounter")"#
    } else {
        "String"
    };
    let sdl = format!(
        r#"type Texts {{
            text: {text}
            vector: [Float32!] @embedding(fields: ["text", "suffix"], provider: "openai", model: "test-model", url: "{}")
            suffix: String
        }}"#,
        server.url()
    );
    let collection = db
        .add_collection(&defra_core::RequestContext::ambient(), &sdl)
        .await
        .unwrap()
        .remove(0);
    Fixture {
        db,
        collection,
        _server: server,
    }
}

/// The embedding `doc` holds, whatever float width carries it: a document
/// just embedded holds the provider's f64 values, and one read back from the
/// store holds its `[Float32!]` field's f32 values.
fn vector(doc: &Document) -> Option<Vec<f64>> {
    match doc.get("vector")? {
        NormalValue::Float64Array(values) => Some(values.clone()),
        NormalValue::Float32Array(values) => Some(values.iter().copied().map(f64::from).collect()),
        _ => None,
    }
}

fn assert_vector(doc: &Document, values: Vec<f64>) {
    assert_eq!(vector(doc), Some(values));
}

#[test]
fn update_regenerates_embedding_from_reloaded_document() {
    workers::block_on(async {
        let fixture = fixture_with_counter(false).await;
        let mutator = AutoCommitMutator::new(Arc::clone(&fixture.db));
        let created = mutator
            .create("Texts", fixture.document("first"))
            .await
            .unwrap();
        let mut stale = created.document.clone();
        stale.set("text", "second").unwrap();
        let mut concurrent = created.document;
        concurrent.set("suffix", "new").unwrap();
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
    })
    .unwrap()
}

#[test]
fn update_embeds_the_applied_counter_value() {
    workers::block_on(async {
        let fixture = fixture_with_counter(true).await;
        let mutator = AutoCommitMutator::new(Arc::clone(&fixture.db));
        let mut doc = Document::new(fixture.collection.clone());
        doc.set("text", 5_i64).unwrap();
        doc.set("suffix", "old").unwrap();
        let created = mutator.create("Texts", doc).await.unwrap();
        let mut concurrent = created.document.clone();
        concurrent.set("text", 6_i64).unwrap();
        concurrent
            .edit(
                "text".to_string(),
                document::FieldOp::Increment(NormalValue::Int(1)),
            )
            .unwrap();
        mutator
            .update(
                "Texts",
                concurrent,
                ["text".to_string()].into_iter().collect(),
            )
            .await
            .unwrap();
        let mut stale = created.document;
        stale.set("text", 7_i64).unwrap();
        stale
            .edit(
                "text".to_string(),
                document::FieldOp::Increment(NormalValue::Int(2)),
            )
            .unwrap();
        let updated = mutator
            .update("Texts", stale, ["text".to_string()].into_iter().collect())
            .await
            .unwrap();
        assert_eq!(updated.document.get("text"), Some(&NormalValue::Int(8)));
        assert_vector(&updated.document, vec![11.0, 12.0]);
        let stored = mutator
            .get_for_update("Texts", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.get("text"), Some(&NormalValue::Int(8)));
        assert_vector(&stored, vec![11.0, 12.0]);
    })
    .unwrap()
}

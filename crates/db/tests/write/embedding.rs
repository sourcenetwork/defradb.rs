use crate::common::fixture::make_test_db_with_bus;
use db::database::DB;
use db::txn::registry::DbTransactionRegistry;
use document::{DocID, Document, NormalValue};
use query::mutator::DocMutator;
use query::txn::TransactionRegistry;
use schema::{CollectionVersion, FieldDescription, FieldKind, VectorEmbeddingDescription};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use storage::RegolithStore;

const STUB_VECTOR: [f64; 3] = [0.25, 0.5, 0.75];

/// Serve an OpenAI-compatible `/embeddings` endpoint that always answers
/// `STUB_VECTOR`, so embedding generation runs without a real provider.
fn spawn_embedding_stub() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let addr = listener.local_addr().expect("stub addr");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                if request_complete(&request) {
                    break;
                }
            }
            let body = serde_json::json!({ "data": [{ "embedding": STUB_VECTOR }] }).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

fn request_complete(request: &[u8]) -> bool {
    let text = String::from_utf8_lossy(request);
    let Some(header_end) = text.find("\r\n\r\n") else {
        return false;
    };
    let content_length = text[..header_end]
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    request.len() >= header_end + 4 + content_length
}

fn embedded_collection(url: &str) -> CollectionVersion {
    let mut collection = CollectionVersion::new(
        "Notes",
        "cv-notes",
        "col-notes",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "content", FieldKind::string()),
            FieldDescription::new("3", "content_v", FieldKind::float64_array()),
        ],
    );
    collection.vector_embeddings = vec![VectorEmbeddingDescription {
        field_name: "content_v".to_string(),
        fields: vec!["content".to_string()],
        model: "stub-model".to_string(),
        provider: "openai".to_string(),
        template: String::new(),
        url: url.to_string(),
    }];
    collection
}

async fn committed_embedding(db: &Arc<DB<RegolithStore>>, doc_id: &DocID) -> Option<NormalValue> {
    let collection = db
        .get_collection("Notes")
        .expect("get collection")
        .expect("collection exists");
    let txn = db.new_txn(true).await.expect("read txn");
    let datastore = txn.datastore().expect("datastore");
    let systemstore = txn.systemstore().expect("systemstore");
    collection
        .get_by_doc_id(&datastore, &systemstore, doc_id)
        .await
        .expect("get doc")
        .expect("doc exists")
        .get("content_v")
        .cloned()
}

#[tokio::test]
async fn autocommit_create_generates_embedding() {
    let (db, _bus) = make_test_db_with_bus().await;
    db.create_collection(embedded_collection(&spawn_embedding_stub()))
        .await
        .expect("schema");

    let mutator = db::AutoCommitMutator::new(Arc::clone(&db));
    let doc = Document::from_json_str(r#"{"content": "hello"}"#).expect("doc");
    let created = mutator.create("Notes", doc).await.expect("create");

    assert_eq!(
        committed_embedding(&db, &created.doc_id).await,
        Some(NormalValue::Float64Array(STUB_VECTOR.to_vec()))
    );
}

#[tokio::test]
async fn explicit_txn_create_generates_embedding() {
    let (db, _bus) = make_test_db_with_bus().await;
    db.create_collection(embedded_collection(&spawn_embedding_stub()))
        .await
        .expect("schema");
    let registry = DbTransactionRegistry::new(Arc::clone(&db));

    let handle = registry.begin(false).await.expect("begin");
    let ctx = registry.get(&handle).into_result().unwrap().unwrap();
    let mutator = ctx.doc_mutator().expect("mutator");
    let doc = Document::from_json_str(r#"{"content": "hello"}"#).expect("doc");
    let created = mutator.create("Notes", doc).await.expect("create");
    drop(mutator);
    drop(ctx);
    registry.commit(&handle).await.expect("commit");

    assert_eq!(
        committed_embedding(&db, &created.doc_id).await,
        Some(NormalValue::Float64Array(STUB_VECTOR.to_vec()))
    );
}

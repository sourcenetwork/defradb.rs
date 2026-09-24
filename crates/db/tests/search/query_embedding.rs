use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};
use db::search::embedding::{embed_text, embed_text_with_provider};
use db::{DenseHybridSearchRequest, EmbeddingClientConfig};
use serde_json::{json, Value};

async fn fixture() -> (
    String,
    tokio::sync::mpsc::UnboundedReceiver<(HeaderMap, Value)>,
    tokio::task::JoinHandle<()>,
) {
    type Sender = tokio::sync::mpsc::UnboundedSender<(HeaderMap, Value)>;
    async fn respond(
        State(sender): State<Sender>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        let ollama = body.get("prompt").is_some();
        sender.send((headers, body)).unwrap();
        Json(if ollama {
            json!({"embedding": [3.0, 4.0]})
        } else {
            json!({"data": [{"embedding": [3.0, 4.0]}]})
        })
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/api", listener.local_addr().unwrap());
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/api/embeddings", post(respond))
                .with_state(sender),
        )
        .await
        .unwrap();
    });
    (url, receiver, server)
}

#[tokio::test]
async fn query_embeddings_share_document_provider_contracts() {
    let (url, mut requests, server) = fixture().await;
    let config = EmbeddingClientConfig::new()
        .with_url(url)
        .with_model("default-model")
        .with_api_key("secret");
    assert_eq!(
        embed_text(&config, "question", None).await.unwrap(),
        vec![3.0, 4.0]
    );
    let (headers, body) = requests.try_recv().unwrap();
    assert_eq!(headers["authorization"], "Bearer secret");
    assert_eq!(body, json!({"input": "question", "model": "default-model"}));

    assert_eq!(
        embed_text_with_provider(&config, "ollama", "question", Some("local-model"))
            .await
            .unwrap(),
        vec![0.6, 0.8]
    );
    let (headers, body) = requests.try_recv().unwrap();
    assert!(!headers.contains_key("authorization"));
    assert_eq!(body, json!({"prompt": "question", "model": "local-model"}));
    let error = embed_text_with_provider(&config, "unknown", "question", None)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("unsupported embedding provider"));
    assert!(requests.try_recv().is_err());
    server.abort();
}

#[tokio::test]
async fn hybrid_search_selects_endpoint_without_forwarding_node_credentials() {
    use db::{DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB};
    use query::QueryExecutor;

    let db = Arc::new(DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
    for schema in query::parse_sdl("type Docs { text: String vector: [Float!] }").unwrap() {
        db.create_collection(schema).await.unwrap();
    }
    let runner = query::QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(db.clone()),
        DbCollectionProvider::new_arc(db.clone()),
        Arc::new(DbTransactionRegistry::new(db.clone())),
    )
    .with_mutator(Arc::new(db::AutoCommitMutator::new(db)));
    let created = runner
        .execute(query::QueryRequest::new(
            r#"mutation { add_Docs(input: {text: "question", vector: [1, 0]}) { _docID } }"#,
        ))
        .await;
    assert!(!created.has_errors(), "{:?}", created.errors);
    let (url, mut requests, server) = fixture().await;
    let config = EmbeddingClientConfig::new()
        .with_url("http://127.0.0.1:1")
        .with_model("node-model")
        .with_api_key("node-secret");
    for provider in ["openai", "ollama"] {
        let request = DenseHybridSearchRequest::new("Docs", "question", "vector", ["text"])
            .with_embedding_provider(provider)
            .with_embedding_url(&url)
            .with_embedding_model("field-model");
        let result = db::hybrid_search_dense(&runner, &config, &request)
            .await
            .unwrap();
        assert_eq!(result.embedding_model, "field-model");
        assert_eq!(result.query_vector_dimensions, 2);
        assert_eq!(result.hits.len(), 1);
        assert!(
            (result.hits[0].dense_score - 0.6).abs() < 1e-6,
            "{provider}: {result:?}"
        );
        let (headers, body) = requests.try_recv().unwrap();
        assert!(!headers.contains_key("authorization"));
        assert_eq!(body["model"], "field-model");
        assert_eq!(
            body[if provider == "ollama" {
                "prompt"
            } else {
                "input"
            }],
            "question"
        );
    }
    assert_eq!(config.api_key, "node-secret");
    let same_endpoint = config.clone().with_url(&url);
    let request = DenseHybridSearchRequest::new("Docs", "question", "vector", ["text"]);
    db::hybrid_search_dense(&runner, &same_endpoint, &request)
        .await
        .unwrap();
    let (headers, body) = requests.try_recv().unwrap();
    assert_eq!(headers["authorization"], "Bearer node-secret");
    assert_eq!(body, json!({"input": "question", "model": "node-model"}));
    server.abort();
}

#[test]
fn existing_hybrid_requests_keep_default_provider_and_endpoint() {
    let request = DenseHybridSearchRequest::new("Docs", "question", "vector", ["text"]);
    let mut value = serde_json::to_value(&request).unwrap();
    value.as_object_mut().unwrap().remove("embedding_provider");
    value.as_object_mut().unwrap().remove("embedding_url");
    let restored: DenseHybridSearchRequest = serde_json::from_value(value).unwrap();
    assert_eq!(restored, request);
}

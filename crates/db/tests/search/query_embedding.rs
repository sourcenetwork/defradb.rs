use std::io::Read;
use std::sync::Arc;

use db::search::embedding::{embed_text, embed_text_with_provider};
use db::{DenseHybridSearchRequest, EmbeddingClientConfig};
use regolith::sync::{unbounded, UnboundedReceiver};
use serde_json::json;

use crate::http::{HttpServer, Request};

fn fixture() -> (String, UnboundedReceiver<Request>, HttpServer) {
    let (sender, receiver) = unbounded();
    let server = HttpServer::start(&["/api/embeddings"], move |request| {
        let ollama = request.body.get("prompt").is_some();
        sender.try_send(request).ok().unwrap();
        if ollama {
            json!({"embedding": [3.0, 4.0]})
        } else {
            json!({"data": [{"embedding": [3.0, 4.0]}]})
        }
    });
    (format!("{}/api", server.url()), receiver, server)
}

#[test]
fn unresponsive_embedding_provider_times_out() {
    workers::block_on(async {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let config = EmbeddingClientConfig::new()
            .with_url(format!("http://{}", listener.local_addr().unwrap()))
            .with_model("model")
            .with_timeout(std::time::Duration::from_millis(200));
        let request =
            edge::testing::edge().spawn(async move { embed_text(&config, "question", None).await });
        let accepted = std::thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            let mut bytes = [0; 4096];
            assert!(connection.read(&mut bytes).unwrap() > 0);
            connection
        });
        let result = workers::timeout(std::time::Duration::from_secs(10), request)
            .await
            .expect("the request deadline must expire without a response")
            .unwrap();
        assert!(result.is_err(), "an unresponsive provider must not succeed");
        drop(accepted.join().unwrap());
    })
    .unwrap()
}

#[test]
fn query_embeddings_share_document_provider_contracts() {
    edge::testing::edge()
        .block_on(async {
            let (url, requests, _server) = fixture();
            let config = EmbeddingClientConfig::new()
                .with_url(url)
                .with_model("default-model")
                .with_api_key("secret");
            assert_eq!(
                embed_text(&config, "question", None).await.unwrap(),
                vec![3.0, 4.0]
            );
            let request = requests.try_recv().unwrap();
            assert_eq!(request.header("authorization"), Some("Bearer secret"));
            assert_eq!(
                request.body,
                json!({"input": "question", "model": "default-model"})
            );

            assert_eq!(
                embed_text_with_provider(&config, "ollama", "question", Some("local-model"))
                    .await
                    .unwrap(),
                vec![0.6, 0.8]
            );
            let request = requests.try_recv().unwrap();
            assert!(request.header("authorization").is_none());
            assert_eq!(
                request.body,
                json!({"prompt": "question", "model": "local-model"})
            );
            let error = embed_text_with_provider(&config, "unknown", "question", None)
                .await
                .unwrap_err();
            assert!(error.to_string().contains("unsupported embedding provider"));
            assert!(requests.try_recv().is_none());
        })
        .unwrap()
}

#[test]
fn hybrid_search_selects_endpoint_without_forwarding_node_credentials() {
    workers::block_on(async {
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
        let (url, requests, _server) = fixture();
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
            let request = requests.try_recv().unwrap();
            assert!(request.header("authorization").is_none());
            assert_eq!(request.body["model"], "field-model");
            assert_eq!(
                request.body[if provider == "ollama" {
                    "prompt"
                } else {
                    "input"
                }],
                "question"
            );
        }
        assert_eq!(config.api_key, "node-secret");
        let same_endpoint = config.clone().with_url(&url);
        for override_url in [None, Some(url.clone()), Some(format!("  {url}//  "))] {
            let mut request = DenseHybridSearchRequest::new("Docs", "question", "vector", ["text"]);
            request.embedding_url = override_url;
            db::hybrid_search_dense(&runner, &same_endpoint, &request)
                .await
                .unwrap();
            let request = requests.try_recv().unwrap();
            assert_eq!(request.header("authorization"), Some("Bearer node-secret"));
            assert_eq!(
                request.body,
                json!({"input": "question", "model": "node-model"})
            );
        }
    })
    .unwrap()
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

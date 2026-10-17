use db::search::embedding::ResolvedEmbeddingConfig;
use db::search::embedding::*;
use db::search::EmbeddingClientConfig;
use document::Document;
use schema::VectorEmbeddingDescription;

fn embedding_with(url: &str, model: &str) -> VectorEmbeddingDescription {
    embedding_with_provider("openai", url, model)
}

fn embedding_with_provider(provider: &str, url: &str, model: &str) -> VectorEmbeddingDescription {
    VectorEmbeddingDescription {
        field_name: "content_v".to_string(),
        fields: vec!["content".to_string()],
        model: model.to_string(),
        provider: provider.to_string(),
        template: String::new(),
        url: url.to_string(),
    }
}

#[test]
fn resolve_embedding_config_prefers_schema_values() {
    let embedding = embedding_with("https://schema.example/v1", "schema-model");
    let config = EmbeddingClientConfig::new()
        .with_url("https://node.example/v1")
        .with_model("node-model")
        .with_api_key("secret");

    let resolved = resolve_embedding_config(&embedding, &config).unwrap();

    assert_eq!(
        resolved,
        ResolvedEmbeddingConfig {
            url: "https://schema.example/v1",
            model: "schema-model",
        }
    );
}

#[test]
fn resolve_embedding_config_falls_back_to_node_values() {
    let embedding = embedding_with("", "");
    let config = EmbeddingClientConfig::new()
        .with_url("https://node.example/v1")
        .with_model("node-model");

    let resolved = resolve_embedding_config(&embedding, &config).unwrap();

    assert_eq!(
        resolved,
        ResolvedEmbeddingConfig {
            url: "https://node.example/v1",
            model: "node-model",
        }
    );
}

#[test]
fn resolve_embedding_config_uses_openai_default_url() {
    let embedding = embedding_with("", "schema-model");
    let config = EmbeddingClientConfig::new();

    assert_eq!(
        resolve_embedding_config(&embedding, &config).unwrap(),
        ResolvedEmbeddingConfig {
            url: "https://api.openai.com/v1",
            model: "schema-model",
        }
    );
}

#[test]
fn resolve_embedding_config_uses_ollama_default_url() {
    let embedding = embedding_with_provider("ollama", "", "schema-model");
    let config = EmbeddingClientConfig::new();

    assert_eq!(
        resolve_embedding_config(&embedding, &config).unwrap(),
        ResolvedEmbeddingConfig {
            url: "http://localhost:11434/api",
            model: "schema-model",
        }
    );
}

#[test]
fn resolve_embedding_config_requires_model() {
    let embedding = embedding_with("https://schema.example/v1", "");
    let config = EmbeddingClientConfig::new();

    assert_eq!(
        resolve_embedding_config(&embedding, &config),
        Err(MissingEmbeddingConfig::Model)
    );
}

#[test]
fn parse_embedding_vector_errors_on_non_numeric_value() {
    let values = vec![serde_json::json!(1.0), serde_json::json!("bad")];

    let err = parse_embedding_vector(&values).unwrap_err();

    assert_eq!(err, "embedding value at index 1 is not numeric");
}

#[test]
fn set_embedding_skips_when_effective_config_is_missing() {
    workers::block_on(async {
        let embeddings = vec![embedding_with_provider("unknown", "", "")];
        let mut doc = Document::new();
        doc.set("content", "hello");

        let generated = set_embedding(
            &embeddings,
            &mut doc,
            true,
            None,
            &EmbeddingClientConfig::new(),
        )
        .await
        .unwrap();

        assert!(generated.is_empty());
        assert!(doc.get("content_v").is_none());
    })
    .unwrap()
}

#[test]
fn set_embedding_uses_each_field_provider_and_model() {
    workers::block_on(async {
        use crate::http::HttpServer;
        use serde_json::json;

        let (requests, received) = regolith::sync::unbounded();
        let server = HttpServer::start(move |request| {
            let (provider, response) = if request.path.starts_with("/openai") {
                ("openai", json!({"data": [{"embedding": [3.0, 4.0]}]}))
            } else {
                ("ollama", json!({"embedding": [0.0, 2.0]}))
            };
            requests.try_send((provider, request)).ok().unwrap();
            response
        });
        let address = server.url();

        let openai = embedding_with_provider(
            "openai",
            &format!("{address}/openai"),
            "text-embedding-3-small",
        );
        let mut ollama =
            embedding_with_provider("ollama", &format!("{address}/ollama"), "nomic-embed-text");
        ollama.field_name = "summary_v".to_string();
        ollama.fields = vec!["summary".to_string()];

        let mut doc = Document::new();
        doc.set("content", "hello");
        doc.set("summary", "world");

        let generated = set_embedding(
            &[openai, ollama],
            &mut doc,
            true,
            None,
            &EmbeddingClientConfig::new()
                .with_url(format!("{address}/unused"))
                .with_model("unused-model")
                .with_api_key("test-key"),
        )
        .await
        .unwrap();
        drop(server);

        let mut requests: Vec<_> = std::iter::from_fn(|| received.try_recv()).collect();
        requests.sort_by_key(|(provider, _)| *provider);
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].0, "ollama");
        assert!(requests[0].1.header("authorization").is_none());
        assert_eq!(
            requests[0].1.body,
            json!({"model": "nomic-embed-text", "prompt": "world\n"})
        );
        assert_eq!(requests[1].0, "openai");
        assert_eq!(
            requests[1].1.header("authorization"),
            Some("Bearer test-key")
        );
        assert_eq!(
            requests[1].1.body,
            json!({"model": "text-embedding-3-small", "input": "hello\n"})
        );

        assert_eq!(generated, vec!["content_v", "summary_v"]);
        assert_eq!(
            doc.get("content_v"),
            Some(&document::NormalValue::Float64Array(vec![3.0, 4.0]))
        );
        assert_eq!(
            doc.get("summary_v"),
            Some(&document::NormalValue::Float64Array(vec![0.0, 1.0]))
        );
    })
    .unwrap()
}

use crate::error::{Error, Result as DbResult};
use crate::search::EmbeddingClientConfig;
use document::{Document, NormalValue};
use rapidhash::RapidHashSet;
use schema::VectorEmbeddingDescription;
use serde::{Deserialize, Serialize};
use tracing::warn;

// HTTP: nuclei sockets natively, the browser's fetch on wasm32. Both have
// the same names.
#[cfg(target_arch = "wasm32")]
use n0_future::time::timeout as with_deadline;
#[cfg(not(target_arch = "wasm32"))]
use net::http as http_client;
#[cfg(not(target_arch = "wasm32"))]
use nuclei::time::timeout as with_deadline;
#[cfg(target_arch = "wasm32")]
use reqwest as http_client;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434/api";
const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";

#[cfg(not(target_arch = "wasm32"))]
type EmbeddingError = Box<dyn std::error::Error + Send + Sync>;
#[cfg(target_arch = "wasm32")]
type EmbeddingError = Box<dyn std::error::Error>;

/// Generate and set embedding vectors on a document based on the collection's
/// vector embedding configuration.
///
/// Returns the names of fields that were generated (so the caller can add them
/// to modified_fields for block creation).
///
/// On create: generates embeddings for all configured fields unless the user
/// already provided the vector value. Runs BEFORE doc ID generation so the
/// content-addressed ID includes embedding values.
///
/// On update: generates embeddings only if a source field was modified.
/// Skips generation if the user explicitly set the vector field.
///
/// The requests go through the process's embedding service, on its network
/// runtime, never from the calling task.
pub async fn set_embedding(
    embeddings: &[VectorEmbeddingDescription],
    doc: &mut Document,
    is_create: bool,
    modified_fields: Option<&RapidHashSet<String>>,
    embedding_config: &EmbeddingClientConfig,
) -> Result<Vec<String>, EmbeddingError> {
    set_embedding_with(
        &crate::remote_inputs::shared_embeddings(),
        embeddings,
        doc,
        is_create,
        modified_fields,
        embedding_config,
    )
    .await
}

/// [`set_embedding`] through `service`: a database's own.
pub(crate) async fn set_embedding_with(
    service: &inputs::Service<crate::remote_inputs::Embeddings>,
    embeddings: &[VectorEmbeddingDescription],
    doc: &mut Document,
    is_create: bool,
    modified_fields: Option<&RapidHashSet<String>>,
    embedding_config: &EmbeddingClientConfig,
) -> Result<Vec<String>, EmbeddingError> {
    let wanted = embedding_keys(
        embeddings,
        doc,
        is_create,
        modified_fields,
        embedding_config,
    );
    if wanted.is_empty() {
        return Ok(Vec::new());
    }

    // Every field's text goes out at once.
    let (fields, keys): (Vec<_>, Vec<_>) = wanted.into_iter().unzip();
    let vectors = service.get_many(keys).await;
    let mut generated = Vec::with_capacity(fields.len());
    for (field, vector) in fields.into_iter().zip(vectors) {
        let vector = vector.map_err(crate::remote_inputs::embedding_error)?;
        doc.set(&field, NormalValue::Float64Array(vector.to_vec()))
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
        generated.push(field);
    }
    Ok(generated)
}

/// The embeddings `doc` needs generated, each as its field and the key the
/// embedding service answers it by.
fn embedding_keys(
    embeddings: &[VectorEmbeddingDescription],
    doc: &Document,
    is_create: bool,
    modified_fields: Option<&RapidHashSet<String>>,
    embedding_config: &EmbeddingClientConfig,
) -> Vec<(String, crate::remote_inputs::Embedding)> {
    let mut wanted = Vec::new();
    for embedding in embeddings {
        // Skip if user explicitly provided the embedding vector
        if is_create {
            if let Some(fv) = doc.get_field_value(&embedding.field_name) {
                if fv.is_dirty() {
                    continue;
                }
            }
        } else if let Some(fields) = modified_fields {
            if fields.contains(&embedding.field_name) {
                continue;
            }
        }

        // On update, check if any source field was modified
        if !is_create {
            if let Some(fields) = modified_fields {
                let any_source_modified = embedding.fields.iter().any(|f| fields.contains(f));
                if !any_source_modified {
                    continue;
                }
            }
        }

        let resolved_config = match resolve_embedding_config(embedding, embedding_config) {
            Ok(config) => config,
            // Missing runtime config is non-fatal by design: collection schemas may
            // declare embeddings while node-level defaults are intentionally absent.
            Err(MissingEmbeddingConfig::Url) => {
                warn!(
                    field = %embedding.field_name,
                    "embedding URL is empty, skipping embedding generation"
                );
                continue;
            }
            Err(MissingEmbeddingConfig::Model) => {
                warn!(
                    field = %embedding.field_name,
                    "embedding model is empty, skipping embedding generation"
                );
                continue;
            }
        };

        let input = EmbeddingInput {
            provider: embedding.provider.clone(),
            url: resolved_config.url.to_string(),
            model: resolved_config.model.to_string(),
            api_key: embedding_config.api_key.clone(),
            text: source_text(embedding, doc),
        };
        wanted.push((
            embedding.field_name.clone(),
            crate::remote_inputs::Embedding::new(input),
        ));
    }
    wanted
}

/// Asks `service` at once for the embeddings every document of `docs` needs
/// on create, so their answers wait in its cache when each document is
/// prepared: one round trip per batch of texts, not one per document. A
/// failure is left for the document's own request to report.
pub(crate) async fn prefetch_embeddings(
    service: &inputs::Service<crate::remote_inputs::Embeddings>,
    embeddings: &[VectorEmbeddingDescription],
    docs: &[Document],
    embedding_config: &EmbeddingClientConfig,
) {
    let keys: Vec<_> = docs
        .iter()
        .flat_map(|doc| embedding_keys(embeddings, doc, true, None, embedding_config))
        .map(|(_, key)| key)
        .collect();
    if keys.len() > 1 {
        service.get_many(keys).await;
    }
}

/// The text an embedding embeds: each source field's value, one per line.
fn source_text(embedding: &VectorEmbeddingDescription, doc: &Document) -> String {
    let mut text = String::new();
    for field in &embedding.fields {
        if let Some(value) = doc.get(field) {
            text.push_str(&normal_value_to_string(value));
            text.push('\n');
        }
    }
    text
}

/// Apply only exact memoized embedding responses inside a transaction attempt.
pub(crate) fn set_embedding_from_inputs(
    embeddings: &[VectorEmbeddingDescription],
    doc: &mut Document,
    is_create: bool,
    modified_fields: &RapidHashSet<String>,
    config: &EmbeddingClientConfig,
    inputs: &query::prepared::PreparedInputMemo,
) -> query::error::Result<Vec<String>> {
    let mut generated = Vec::new();
    let mut first_miss = None;
    for embedding in embeddings {
        if is_create
            && doc
                .get_field_value(&embedding.field_name)
                .is_some_and(|field| field.is_dirty())
        {
            continue;
        }
        if !is_create && modified_fields.contains(&embedding.field_name)
            || !is_create
                && !embedding
                    .fields
                    .iter()
                    .any(|field| modified_fields.contains(field))
            || resolve_embedding_config(embedding, config).is_err()
        {
            continue;
        }
        let request = prepared_embedding_request(embedding, doc, config)
            .map_err(query::error::QueryError::execution)?;
        // A miss is noted and the other fields go on: every embedding the
        // document lacks is asked for in one wave.
        let Some(response) = inputs.get_or_want(&request) else {
            first_miss.get_or_insert(request);
            continue;
        };
        let vector: Vec<f64> = serde_json::from_slice(&response).map_err(|error| {
            query::error::QueryError::execution(format!("invalid prepared embedding: {error}"))
        })?;
        doc.set(&embedding.field_name, NormalValue::Float64Array(vector))
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
        generated.push(embedding.field_name.clone());
    }
    match first_miss {
        Some(request) => Err(query::error::QueryError::NeedsInput(request)),
        None => Ok(generated),
    }
}

/// Exact provider request needed to resolve an embedding after an attempt
/// discovered newer source fields. The API key stays in this internal byte
/// request and is redacted by `PreparedInputRequest::Debug`.
#[derive(Serialize, Deserialize)]
struct PreparedEmbeddingRequest {
    provider: String,
    url: String,
    model: String,
    api_key: String,
    text: String,
}

pub(crate) fn prepared_embedding_request(
    embedding: &VectorEmbeddingDescription,
    doc: &Document,
    config: &EmbeddingClientConfig,
) -> Result<query::prepared::PreparedInputRequest, String> {
    let resolved =
        resolve_embedding_config(embedding, config).map_err(|missing| match missing {
            MissingEmbeddingConfig::Url => "embedding URL is empty".to_string(),
            MissingEmbeddingConfig::Model => "embedding model is empty".to_string(),
        })?;
    let request = PreparedEmbeddingRequest {
        provider: embedding.provider.clone(),
        url: resolved.url.to_string(),
        model: resolved.model.to_string(),
        api_key: config.api_key.clone(),
        text: source_text(embedding, doc),
    };
    let bytes = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    Ok(query::prepared::PreparedInputRequest::new(
        query::prepared::PreparedInputKind::Embedding,
        bytes,
    ))
}

/// One prepared embedding request, straight to the provider: the path the
/// prepared-input test drives. A database resolves through its embedding
/// service instead.
#[cfg(test)]
pub(crate) async fn resolve_prepared_embedding_request(request: &[u8]) -> Result<Vec<u8>, String> {
    let request = EmbeddingInput::decode(request)?;
    let embedding = call_embedding(
        &request.provider,
        &request.url,
        &request.model,
        &request.api_key,
        &request.text,
        EMBEDDING_TIMEOUT,
    )
    .await
    .map_err(|error| error.to_string())?;
    serde_json::to_vec(&embedding).map_err(|error| error.to_string())
}

/// One embedding to make, as a prepared request carries it.
#[derive(Clone, Debug)]
pub(crate) struct EmbeddingInput {
    pub provider: String,
    pub url: String,
    pub model: String,
    pub api_key: String,
    pub text: String,
}

impl EmbeddingInput {
    /// The input a prepared embedding request names.
    pub(crate) fn decode(request: &[u8]) -> Result<Self, String> {
        let request: PreparedEmbeddingRequest =
            serde_json::from_slice(request).map_err(|error| error.to_string())?;
        Ok(Self {
            provider: request.provider,
            url: request.url,
            model: request.model,
            api_key: request.api_key,
            text: request.text,
        })
    }
}

/// Embed free-form text using DefraDB's v1 embedding contract.
///
/// This expects an OpenAI-compatible `/embeddings` endpoint and sends:
/// `{ "model": "...", "input": "..." }`.
pub async fn embed_text(
    embedding_config: &EmbeddingClientConfig,
    text: &str,
    model: Option<&str>,
) -> DbResult<Vec<f64>> {
    embed_text_with_provider(embedding_config, "openai", text, model).await
}

/// Embed query text using the same provider contract as document embeddings.
pub async fn embed_text_with_provider(
    embedding_config: &EmbeddingClientConfig,
    provider: &str,
    text: &str,
    model: Option<&str>,
) -> DbResult<Vec<f64>> {
    let input = query_input(embedding_config, provider, text, model).map_err(Error::other)?;
    call_embedding(
        &input.provider,
        &input.url,
        &input.model,
        &input.api_key,
        &input.text,
        embedding_config.timeout.unwrap_or(EMBEDDING_TIMEOUT),
    )
    .await
    .map_err(Error::from)
}

/// A query text's embedding, through the process's embedding service on
/// its network runtime: never cached, so each query asks its provider.
pub(crate) async fn embed_query(
    embedding_config: &EmbeddingClientConfig,
    provider: &str,
    text: &str,
    model: Option<&str>,
) -> Result<Vec<f64>, EmbeddingCallError> {
    let input = query_input(embedding_config, provider, text, model)
        .map_err(EmbeddingCallError::final_error)?;
    crate::remote_inputs::shared_embeddings()
        .get(crate::remote_inputs::Embedding::query(input))
        .await
        .map(|vector| vector.to_vec())
        .map_err(crate::remote_inputs::embedding_error)
}

/// What a query's text embeds with: the endpoint the config names and the
/// model the query names, else the config's.
fn query_input(
    embedding_config: &EmbeddingClientConfig,
    provider: &str,
    text: &str,
    model: Option<&str>,
) -> Result<EmbeddingInput, &'static str> {
    let url = embedding_config.url.trim();
    if url.is_empty() {
        return Err("embedding URL is empty");
    }
    let model = model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .or_else(|| {
            let default_model = embedding_config.model.trim();
            (!default_model.is_empty()).then_some(default_model)
        })
        .ok_or("embedding model is empty")?;
    Ok(EmbeddingInput {
        provider: provider.to_string(),
        url: url.to_string(),
        model: model.to_string(),
        api_key: embedding_config.api_key.clone(),
        text: text.to_string(),
    })
}

fn normal_value_to_string(val: &NormalValue) -> String {
    match val {
        NormalValue::String(s) => s.clone(),
        NormalValue::Int(i) => i.to_string(),
        NormalValue::Float64(f) => format!("{}", f),
        NormalValue::Float32(f) => format!("{}", f),
        NormalValue::Bool(b) => b.to_string(),
        other => format!("{:?}", other),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MissingEmbeddingConfig {
    Url,
    Model,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResolvedEmbeddingConfig<'a> {
    pub url: &'a str,
    pub model: &'a str,
}

pub fn resolve_embedding_config<'a>(
    embedding: &'a VectorEmbeddingDescription,
    embedding_config: &'a EmbeddingClientConfig,
) -> Result<ResolvedEmbeddingConfig<'a>, MissingEmbeddingConfig> {
    let url = if !embedding.url.is_empty() {
        embedding.url.as_str()
    } else if !embedding_config.url.is_empty() {
        embedding_config.url.as_str()
    } else {
        match embedding.provider.as_str() {
            "ollama" => DEFAULT_OLLAMA_URL,
            "openai" => DEFAULT_OPENAI_URL,
            _ => return Err(MissingEmbeddingConfig::Url),
        }
    };

    let model = if embedding.model.is_empty() {
        embedding_config.model.as_str()
    } else {
        embedding.model.as_str()
    };
    if model.is_empty() {
        return Err(MissingEmbeddingConfig::Model);
    }

    Ok(ResolvedEmbeddingConfig { url, model })
}

/// The form every embedding URL is used in: surrounding whitespace gone and
/// trailing slashes dropped, so `https://host/api/` and `https://host/api`
/// are one endpoint everywhere a URL is compared or joined.
pub(crate) fn normalized_embedding_url(url: &str) -> &str {
    url.trim().trim_end_matches('/')
}

/// How long one embedding request may take unless the config sets another.
pub(crate) const EMBEDDING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Why an embedding request failed, and whether sending it again may
/// succeed: a timeout, a transport error, 429 or 5xx.
#[derive(Clone, Debug)]
pub(crate) struct EmbeddingCallError {
    pub message: String,
    pub retryable: bool,
}

impl EmbeddingCallError {
    pub(crate) fn final_error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }
}

impl std::fmt::Display for EmbeddingCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EmbeddingCallError {}

impl From<EmbeddingCallError> for Error {
    fn from(error: EmbeddingCallError) -> Self {
        Error::other(error.to_string())
    }
}

/// Ollama's `prompt` form embeds one text per request: this many at once.
const SINGLE_TEXT_CONCURRENCY: usize = 16;

async fn call_embedding(
    provider: &str,
    url: &str,
    model: &str,
    api_key: &str,
    text: &str,
    timeout: std::time::Duration,
) -> Result<Vec<f64>, EmbeddingCallError> {
    call_embeddings(provider, url, model, api_key, &[text], timeout)
        .await?
        .pop()
        .unwrap_or_else(|| {
            Err(EmbeddingCallError::final_error(
                "embedding response missing /data/0/embedding",
            ))
        })
}

/// Embeds `texts` at one endpoint, one answer per text in order: one
/// request carrying every text where the API takes an array
/// (OpenAI-compatible: a single text goes as a string, several as an
/// array), one request per text, [`SINGLE_TEXT_CONCURRENCY`] at a time,
/// where it does not (Ollama's `prompt` form). The error outside the
/// answers fails every text: the request did not get through.
pub(crate) async fn call_embeddings(
    provider: &str,
    url: &str,
    model: &str,
    api_key: &str,
    texts: &[&str],
    timeout: std::time::Duration,
) -> Result<Vec<Result<Vec<f64>, EmbeddingCallError>>, EmbeddingCallError> {
    let endpoint = format!("{}/embeddings", normalized_embedding_url(url));
    match provider {
        "openai" => {
            let input = match texts {
                [text] => serde_json::json!(text),
                texts => serde_json::json!(texts),
            };
            let bearer = (!api_key.is_empty()).then_some(api_key);
            let body = serde_json::json!({ "model": model, "input": input });
            let result = post_embedding_request(&endpoint, &body, bearer, timeout).await?;
            Ok(openai_vectors(&result, texts.len()))
        }
        "ollama" => {
            let mut vectors = Vec::with_capacity(texts.len());
            for chunk in texts.chunks(SINGLE_TEXT_CONCURRENCY) {
                vectors.extend(
                    futures::future::join_all(
                        chunk
                            .iter()
                            .map(|text| ollama_embedding(&endpoint, model, text, timeout)),
                    )
                    .await,
                );
            }
            Ok(vectors)
        }
        _ => Err(EmbeddingCallError::final_error(format!(
            "unsupported embedding provider: {provider}"
        ))),
    }
}

/// One text by Ollama's `prompt` form, normalized to unit length.
async fn ollama_embedding(
    endpoint: &str,
    model: &str,
    text: &str,
    timeout: std::time::Duration,
) -> Result<Vec<f64>, EmbeddingCallError> {
    let body = serde_json::json!({ "model": model, "prompt": text });
    let result = post_embedding_request(endpoint, &body, None, timeout).await?;
    let mut vector = vector_at(&result, "/embedding")?;
    normalize_embedding(&mut vector)
        .map_err(|error| EmbeddingCallError::final_error(error.to_string()))?;
    Ok(vector)
}

/// One embedding request, bounded by `timeout`: the JSON answer,
/// or why there is none.
async fn post_embedding_request(
    endpoint: &str,
    body: &serde_json::Value,
    bearer: Option<&str>,
    timeout: std::time::Duration,
) -> Result<serde_json::Value, EmbeddingCallError> {
    let mut request = embedding_client().post(endpoint);
    if let Some(api_key) = bearer {
        request = request.bearer_auth(api_key);
    }
    let transport = |error: http_client::Error| EmbeddingCallError {
        message: error.to_string(),
        retryable: true,
    };
    let exchange = async {
        let resp = request.json(body).send().await.map_err(transport)?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(EmbeddingCallError {
                message: format!("embedding provider returned {}: {}", status, text),
                retryable: status.as_u16() == 429 || status.is_server_error(),
            });
        }
        resp.json::<serde_json::Value>().await.map_err(transport)
    };
    with_deadline(timeout, exchange)
        .await
        .map_err(|_| EmbeddingCallError {
            message: format!("embedding provider gave no answer within {timeout:?}"),
            retryable: true,
        })?
}

/// The vectors of an OpenAI-compatible answer for `count` inputs, placed by
/// each entry's `index` (by position where an entry has none).
fn openai_vectors(
    result: &serde_json::Value,
    count: usize,
) -> Vec<Result<Vec<f64>, EmbeddingCallError>> {
    let mut vectors: Vec<Option<Result<Vec<f64>, EmbeddingCallError>>> =
        (0..count).map(|_| None).collect();
    let entries = result
        .get("data")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for (position, entry) in entries.iter().enumerate() {
        let index = entry
            .get("index")
            .and_then(serde_json::Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(position);
        if let Some(slot) = vectors.get_mut(index) {
            *slot = Some(vector_at(entry, "/embedding"));
        }
    }
    vectors
        .into_iter()
        .enumerate()
        .map(|(index, vector)| {
            vector.unwrap_or_else(|| {
                Err(EmbeddingCallError::final_error(format!(
                    "embedding response missing /data/{index}/embedding"
                )))
            })
        })
        .collect()
}

/// The numeric array at `pointer` in an answer.
fn vector_at(answer: &serde_json::Value, pointer: &str) -> Result<Vec<f64>, EmbeddingCallError> {
    let values = answer
        .pointer(pointer)
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            EmbeddingCallError::final_error(format!("embedding response missing {pointer}"))
        })?;
    parse_embedding_vector(values).map_err(EmbeddingCallError::final_error)
}

fn normalize_embedding(embedding: &mut [f64]) -> Result<(), EmbeddingError> {
    let magnitude = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if magnitude == 0.0 {
        return Err("embedding response contains a zero vector".into());
    }
    if (magnitude - 1.0).abs() >= 1e-6 {
        for value in embedding {
            *value /= magnitude;
        }
    }
    Ok(())
}

fn embedding_client() -> &'static http_client::Client {
    static CLIENT: regolith::sync::Lazy<http_client::Client> =
        regolith::sync::Lazy::new(http_client::Client::new);
    &CLIENT
}

pub fn parse_embedding_vector(embedding: &[serde_json::Value]) -> Result<Vec<f64>, String> {
    embedding
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_f64()
                .ok_or_else(|| format!("embedding value at index {} is not numeric", index))
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/common/http.rs"]
mod test_http;

#[cfg(test)]
mod prepared_input_tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::test_http::HttpServer;

    /// The request goes out on the network runtime, as it does from a node.
    async fn on_edge(request: &[u8]) -> Vec<u8> {
        let request = request.to_vec();
        edge::testing::edge()
            .spawn(async move { resolve_prepared_embedding_request(&request).await })
            .await
            .unwrap()
            .unwrap()
    }

    #[test]
    fn retries_reuse_exact_embedding_response_and_changed_source_fetches_again() {
        workers::block_on(async {
            let provider_calls = std::sync::Arc::new(AtomicUsize::new(0));
            let call_count = std::sync::Arc::clone(&provider_calls);
            let server = HttpServer::start(&["/embeddings"], move |_| {
                call_count.fetch_add(1, Ordering::SeqCst);
                json!({"data": [{"embedding": [1.0, 2.0]}]})
            });

            let description = VectorEmbeddingDescription {
                field_name: "vector".to_string(),
                fields: vec!["text".to_string()],
                model: "test-model".to_string(),
                provider: "openai".to_string(),
                template: String::new(),
                url: String::new(),
            };
            let config = EmbeddingClientConfig::new()
                .with_url(server.url())
                .with_model("test-model");
            let memo = query::prepared::PreparedInputMemo::default();
            let mut doc = Document::new(crate::common::schema::open_collection(&["text"]));
            doc.set("text", "source A").unwrap();
            let first = prepared_embedding_request(&description, &doc, &config).unwrap();

            for _ in 0..2 {
                if memo.get(&first).is_none() {
                    let bytes = on_edge(first.exact_request()).await;
                    memo.insert(first.clone(), bytes);
                }
            }
            assert_eq!(provider_calls.load(Ordering::SeqCst), 1);

            doc.set("text", "source B").unwrap();
            let changed = prepared_embedding_request(&description, &doc, &config).unwrap();
            assert!(memo.get(&changed).is_none());
            let bytes = on_edge(changed.exact_request()).await;
            memo.insert(changed, bytes);
            assert_eq!(provider_calls.load(Ordering::SeqCst), 2);
        })
        .unwrap()
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;

    #[test]
    fn vectors_are_placed_by_their_index_and_a_missing_one_is_an_error() {
        let answer = serde_json::json!({"data": [
            {"index": 1, "embedding": [2.0]},
            {"index": 0, "embedding": [1.0]},
        ]});
        let vectors = openai_vectors(&answer, 3);
        assert_eq!(vectors[0].as_ref().unwrap(), &vec![1.0]);
        assert_eq!(vectors[1].as_ref().unwrap(), &vec![2.0]);
        let missing = vectors[2].as_ref().unwrap_err();
        assert!(!missing.retryable);
        assert!(missing.message.contains("/data/2/embedding"), "{missing}");
    }

    #[test]
    fn entries_without_an_index_are_placed_by_position() {
        let answer = serde_json::json!({"data": [{"embedding": [1.0]}, {"embedding": [2.0]}]});
        let vectors = openai_vectors(&answer, 2);
        assert_eq!(vectors[0].as_ref().unwrap(), &vec![1.0]);
        assert_eq!(vectors[1].as_ref().unwrap(), &vec![2.0]);
    }
}

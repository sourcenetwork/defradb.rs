//! Embedding requests to a provider: the body each provider's API is sent,
//! a deadline on every request, which failures may be sent again, and the
//! vectors read from the answer.

use std::time::Duration;

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

/// How long one embedding request may take unless the config sets another.
pub const EMBEDDING_TIMEOUT: Duration = Duration::from_secs(30);

/// Ollama's `prompt` form embeds one text per request: this many at once.
const SINGLE_TEXT_CONCURRENCY: usize = 16;

/// Why an embedding request failed, and whether sending it again may
/// succeed: a timeout, a transport error, 429 or 5xx.
#[derive(Clone, Debug, thiserror::Error)]
#[error("{message}")]
pub struct EmbeddingCallError {
    /// What went wrong.
    pub message: String,
    /// Whether sending the request again may succeed.
    pub retryable: bool,
}

impl EmbeddingCallError {
    /// A failure that sending the request again cannot fix.
    pub fn final_error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }
}

/// The HTTP client embedding requests go out on.
pub use http_client::Client;

/// A client for embedding requests whose connections run on `edge`.
pub fn client(edge: &edge::Edge) -> Client {
    #[cfg(not(target_arch = "wasm32"))]
    return Client::new(edge.clone());
    #[cfg(target_arch = "wasm32")]
    {
        let _ = edge;
        Client::new()
    }
}

/// The client on the process's on-demand network runtime (the browser's
/// fetch on wasm32), for embeddings asked with no edge; why there is none
/// when that runtime did not start.
pub fn shared_client() -> Result<Client, EmbeddingCallError> {
    static CLIENT: regolith::sync::OnceCell<Client> = regolith::sync::OnceCell::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    #[cfg(not(target_arch = "wasm32"))]
    let shared = client(edge::on_demand().map_err(|error| EmbeddingCallError {
        message: error.to_string(),
        retryable: true,
    })?);
    #[cfg(target_arch = "wasm32")]
    let shared = Client::new();
    Ok(CLIENT.get_or_init_racy(|| shared).clone())
}

/// The form every embedding URL is used in: surrounding whitespace gone and
/// trailing slashes dropped, so `https://host/api/` and `https://host/api`
/// are one endpoint everywhere a URL is compared or joined.
pub fn normalized_embedding_url(url: &str) -> &str {
    url.trim().trim_end_matches('/')
}

/// Embeds one text at one endpoint.
pub async fn call_embedding(
    client: &Client,
    provider: &str,
    url: &str,
    model: &str,
    api_key: &str,
    text: &str,
    timeout: Duration,
) -> Result<Vec<f64>, EmbeddingCallError> {
    call_embeddings(client, provider, url, model, api_key, &[text], timeout)
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
/// array), one request per text, `SINGLE_TEXT_CONCURRENCY` at a time,
/// where it does not (Ollama's `prompt` form). The error outside the
/// answers fails every text: the request did not get through.
pub async fn call_embeddings(
    client: &Client,
    provider: &str,
    url: &str,
    model: &str,
    api_key: &str,
    texts: &[&str],
    timeout: Duration,
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
            let result = post_embedding_request(client, &endpoint, &body, bearer, timeout).await?;
            Ok(openai_vectors(&result, texts.len()))
        }
        "ollama" => {
            let mut vectors = Vec::with_capacity(texts.len());
            for chunk in texts.chunks(SINGLE_TEXT_CONCURRENCY) {
                vectors.extend(
                    futures::future::join_all(
                        chunk
                            .iter()
                            .map(|text| ollama_embedding(client, &endpoint, model, text, timeout)),
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
    client: &Client,
    endpoint: &str,
    model: &str,
    text: &str,
    timeout: Duration,
) -> Result<Vec<f64>, EmbeddingCallError> {
    let body = serde_json::json!({ "model": model, "prompt": text });
    let result = post_embedding_request(client, endpoint, &body, None, timeout).await?;
    let mut vector = vector_at(&result, "/embedding")?;
    normalize_embedding(&mut vector)?;
    Ok(vector)
}

/// One embedding request, bounded by `timeout`: the JSON answer,
/// or why there is none.
async fn post_embedding_request(
    client: &Client,
    endpoint: &str,
    body: &serde_json::Value,
    bearer: Option<&str>,
    timeout: Duration,
) -> Result<serde_json::Value, EmbeddingCallError> {
    let mut request = client.post(endpoint);
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

fn normalize_embedding(embedding: &mut [f64]) -> Result<(), EmbeddingCallError> {
    let magnitude = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if magnitude == 0.0 {
        return Err(EmbeddingCallError::final_error(
            "embedding response contains a zero vector",
        ));
    }
    if (magnitude - 1.0).abs() >= 1e-6 {
        for value in embedding {
            *value /= magnitude;
        }
    }
    Ok(())
}

/// The numbers of an answer's embedding array, or which entry is not one.
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
mod tests {
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

    #[test]
    fn parse_embedding_vector_errors_on_non_numeric_value() {
        let values = vec![serde_json::json!(1.0), serde_json::json!("bad")];

        let err = parse_embedding_vector(&values).unwrap_err();

        assert_eq!(err, "embedding value at index 1 is not numeric");
    }
}

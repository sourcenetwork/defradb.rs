//! Embeddings as an [`inputs::Source`]: single-flight and a byte-bounded
//! cache keyed by (provider, normalized URL, model, SHA-256 of the text), a
//! deadline on every target, and retries on 429, 5xx and transport errors.
//! A query's embedding is shared while in flight but never cached.
//!
//! A wave's texts for one (provider, URL, model) share one request where the
//! API takes an array (OpenAI-compatible), up to the batch cap; where it does
//! not (Ollama's `prompt` form) they go one request each, concurrently. A
//! wave takes one round trip of time, whatever its size.
//!
//! A model replaced under the same name keeps serving the old vectors from
//! the cache until their time to live ends.

use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use defra_core::thread_bounds::MaybeBoxFuture;
use inputs::embeddings::{
    call_embeddings, normalized_embedding_url, Client, EmbeddingCallError, EMBEDDING_TIMEOUT,
};
use sha2::{Digest, Sha256};

use crate::search::embedding::EmbeddingInput;

/// One embedding: what it is keyed by, and the request that makes it.
#[derive(Clone)]
pub(crate) struct Embedding {
    endpoint: Endpoint,
    digest: [u8; 32],
    /// A query's embedding: never cached, so every query asks its provider.
    fresh: bool,
    input: Arc<EmbeddingInput>,
}

impl Embedding {
    /// A document's embedding, kept in the cache while it stays.
    pub(crate) fn new(input: EmbeddingInput) -> Self {
        Self::keyed(input, false)
    }

    /// A query's embedding: shared while in flight, never cached.
    pub(crate) fn query(input: EmbeddingInput) -> Self {
        Self::keyed(input, true)
    }

    fn keyed(input: EmbeddingInput, fresh: bool) -> Self {
        Self {
            endpoint: Endpoint {
                provider: Arc::from(input.provider.as_str()),
                url: Arc::from(normalized_embedding_url(&input.url)),
                model: Arc::from(input.model.as_str()),
                api_key: Arc::from(input.api_key.as_str()),
            },
            digest: Sha256::digest(input.text.as_bytes()).into(),
            fresh,
            input: Arc::new(input),
        }
    }
}

/// Equal when one vector answers both: the same provider, normalized URL,
/// model and text, both cached or both fresh. The API key opens the request
/// but does not change the vector.
impl PartialEq for Embedding {
    fn eq(&self, other: &Self) -> bool {
        self.endpoint.provider == other.endpoint.provider
            && self.endpoint.url == other.endpoint.url
            && self.endpoint.model == other.endpoint.model
            && self.digest == other.digest
            && self.fresh == other.fresh
    }
}

impl Eq for Embedding {}

impl Hash for Embedding {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.endpoint.provider.hash(state);
        self.endpoint.url.hash(state);
        self.endpoint.model.hash(state);
        self.digest.hash(state);
        self.fresh.hash(state);
    }
}

/// Where an embedding is made and with which key: the texts of one share a
/// request.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct Endpoint {
    provider: Arc<str>,
    url: Arc<str>,
    model: Arc<str>,
    api_key: Arc<str>,
}

/// The embedding source, with the client its requests go out on, or why
/// there is none.
pub(crate) struct Embeddings {
    client: Result<Client, EmbeddingCallError>,
}

impl Embeddings {
    /// Requests on `edge`'s connections.
    pub(crate) fn on(edge: &edge::Edge) -> Self {
        Self {
            client: Ok(inputs::embeddings::client(edge)),
        }
    }
}

pub(super) fn config() -> inputs::Config {
    inputs::Config {
        // Past one request's own timeout, so a retry fits.
        deadline: Duration::from_secs(75),
        attempts: 3,
        backoff: Duration::from_millis(250),
        max_backoff: Duration::from_secs(5),
        // OpenAI takes up to 2048 inputs per request; a smaller batch keeps
        // one request's answer to a few megabytes.
        max_batch: 256,
        flights_per_group: 4,
        max_in_flight: 32,
        cache_bytes: 16 << 20,
        ttl: Some(Duration::from_secs(600)),
    }
}

impl inputs::Source for Embeddings {
    type Key = Embedding;
    type Group = Endpoint;
    type Value = Arc<[f64]>;
    type Error = EmbeddingCallError;

    fn name(&self) -> &'static str {
        "embeddings"
    }

    fn group(&self, key: &Embedding) -> Endpoint {
        key.endpoint.clone()
    }

    fn fetch<'a>(
        &'a self,
        endpoint: &'a Endpoint,
        keys: &'a [Embedding],
    ) -> MaybeBoxFuture<'a, inputs::Answers<Arc<[f64]>, EmbeddingCallError>> {
        Box::pin(async move {
            let client = self.client.as_ref().map_err(Clone::clone)?;
            let texts: Vec<&str> = keys.iter().map(|key| key.input.text.as_str()).collect();
            let vectors = call_embeddings(
                client,
                &endpoint.provider,
                &endpoint.url,
                &endpoint.model,
                &endpoint.api_key,
                &texts,
                EMBEDDING_TIMEOUT,
            )
            .await?;
            Ok(vectors
                .into_iter()
                .map(|vector| vector.map(Arc::from))
                .collect())
        })
    }

    fn retryable(&self, error: &EmbeddingCallError) -> bool {
        error.retryable
    }

    fn cache_bytes(&self, key: &Embedding, value: &Arc<[f64]>) -> Option<usize> {
        (!key.fresh).then(|| {
            value.len() * std::mem::size_of::<f64>()
                + key.endpoint.provider.len()
                + key.endpoint.url.len()
                + key.endpoint.model.len()
                + 32
        })
    }
}

/// The process's embedding service, for a database the node gave no
/// network runtime and for embeddings asked with no database (a query's, or
/// [`set_embedding`](crate::search::set_embedding)'s): on the process's
/// on-demand network runtime ([`inputs::Runtime::on_demand`]).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn shared() -> inputs::Service<Embeddings> {
    static SHARED: regolith::sync::OnceCell<inputs::Service<Embeddings>> =
        regolith::sync::OnceCell::new();
    SHARED.get_or_init_racy(on_demand).clone()
}

/// The browser's embedding service: one per thread, a service there being
/// bound to its thread.
#[cfg(target_arch = "wasm32")]
pub(crate) fn shared() -> inputs::Service<Embeddings> {
    thread_local! {
        static SHARED: inputs::Service<Embeddings> = on_demand();
    }
    SHARED.with(Clone::clone)
}

fn on_demand() -> inputs::Service<Embeddings> {
    let embeddings = Embeddings {
        client: inputs::embeddings::shared_client(),
    };
    inputs::Service::new(embeddings, config(), inputs::Runtime::on_demand())
}

/// Why a service could not embed a text, as the embedding call reports it.
pub(crate) fn embedding_error(error: inputs::Error<EmbeddingCallError>) -> EmbeddingCallError {
    match error {
        inputs::Error::Source(error) => error,
        other => EmbeddingCallError {
            message: other.to_string(),
            retryable: false,
        },
    }
}

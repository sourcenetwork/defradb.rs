//! HTTP client for communicating with DefraDB server

use regolith::sync::OnceCell;
use std::time::Duration;

use net::http::{Client, RequestBuilder, Response, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value as JsonValue;
use url::Url;

use crate::error::{Error, Result};

pub use defra_http::go_paths::{
    ACP_POLICY as GO_ACP_POLICY, VIEW_ADD as GO_VIEW_ADD, VIEW_REFRESH as GO_VIEW_REFRESH,
};

mod acp;
mod backup;
mod block;
mod collection;
mod definitions;
mod documents;
mod dump;
mod encrypted_index;
mod graphql;
mod index;
mod p2p;
mod peer;
mod transaction;
mod types;
mod view;

pub use acp::{AcpAddPolicyRequest, AcpAddPolicyResponse, AcpPolicy, NacRelationshipRequest};
pub use encrypted_index::EncryptedIndexInfo;
pub use index::{IndexFieldInfo, IndexInfo};
pub use p2p::{
    mint_manage_token, ManageQueryResultResponse, P2pReplicatorInfo, P2pReplicatorRequest,
};
pub use types::{ErrorResponse, GraphQLError, GraphQLRequest, GraphQLResponse, TxBeginResponse};
pub use view::{AddViewRequest, ViewRefreshSelectors};

/// Default timeout for HTTP requests (30 seconds)
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default connection timeout (10 seconds)
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Maximum number of retry attempts for transient failures
pub const MAX_RETRIES: u32 = 3;

/// Initial backoff delay in milliseconds
pub const INITIAL_BACKOFF_MS: u64 = 100;

/// HTTP status codes that are considered retryable
pub const RETRYABLE_STATUS_CODES: &[u16] = &[408, 429, 500, 502, 503, 504];

/// Global shared HTTP client for connection reuse across commands
static SHARED_CLIENT: OnceCell<std::result::Result<Client, String>> = OnceCell::new();

fn get_shared_client(edge: &edge::Edge) -> Result<&'static Client> {
    let result = SHARED_CLIENT.get_or_init_racy(|| {
        Client::builder(edge.clone())
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
            .pool_max_idle_per_host(5)
            .build()
            .map_err(|e| format!("{}", e))
    });

    match result {
        Ok(client) => Ok(client),
        Err(msg) => Err(Error::HttpClientInit(msg.clone())),
    }
}

/// Declares a simple endpoint method with no parameters.
macro_rules! endpoint {
    ($name:ident, GET $path:expr => $ret:ty) => {
        pub async fn $name(&self) -> Result<$ret> {
            let url = format!("{}{}", self.base_url, $path);
            self.request_json("GET", &url, None).await
        }
    };
    ($name:ident, POST $path:expr => $ret:ty) => {
        pub async fn $name(&self) -> Result<$ret> {
            let url = format!("{}{}", self.base_url, $path);
            self.request_json("POST", &url, None).await
        }
    };
    ($name:ident, POST $path:expr) => {
        pub async fn $name(&self) -> Result<()> {
            let url = format!("{}{}", self.base_url, $path);
            self.request_void("POST", &url, None).await
        }
    };
}

/// HTTP client for DefraDB server communication
pub struct HttpClient {
    client: &'static Client,
    base_url: String,
    auth_token: Option<String>,
    verbose: bool,
}

impl HttpClient {
    /// Create a new HTTP client with the given base URL, its connections on
    /// `edge`.
    ///
    /// Uses a shared connection pool with configured timeouts for efficiency.
    /// Returns an error if the URL is invalid or the HTTP client fails to initialize.
    pub fn new(base_url: &str, edge: &edge::Edge) -> Result<Self> {
        let normalized_url = base_url.trim_end_matches('/');

        // Validate the URL format
        Url::parse(normalized_url)
            .map_err(|e| Error::InvalidUrl(normalized_url.to_string(), e.to_string()))?;

        Ok(Self {
            client: get_shared_client(edge)?,
            base_url: normalized_url.to_string(),
            auth_token: None,
            verbose: false,
        })
    }

    /// Set the authentication token (JWT Bearer token) for ACP-protected operations
    pub fn with_auth_token(mut self, token: Option<String>) -> Self {
        self.auth_token = token;
        self
    }

    /// Enable verbose mode to print request/response details
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Add authorization header if auth_token is set
    fn add_auth_header(&self, request: RequestBuilder) -> RequestBuilder {
        if let Some(ref token) = self.auth_token {
            request.header("Authorization", format!("Bearer {}", token))
        } else {
            request
        }
    }

    /// Check if a status code is retryable
    pub fn is_retryable_status(status: StatusCode) -> bool {
        RETRYABLE_STATUS_CODES.contains(&status.as_u16())
    }

    /// Get the base URL for the client
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The Go wire path for a policy registration, under the default API
    /// version. Defined by the server crate so the client cannot drift from
    /// the routes it calls.
    pub fn acp_policy_url(&self) -> String {
        format!(
            "{}{}",
            self.base_url,
            defra_http::go_paths::v0(GO_ACP_POLICY)
        )
    }

    /// The Go wire path for adding a view.
    pub fn view_add_url(&self) -> String {
        format!("{}{}", self.base_url, defra_http::go_paths::v0(GO_VIEW_ADD))
    }

    /// The Go wire path for refreshing views.
    pub fn view_refresh_url(&self) -> String {
        format!(
            "{}{}",
            self.base_url,
            defra_http::go_paths::v0(GO_VIEW_REFRESH)
        )
    }

    /// Check if an error is retryable (connection errors, timeouts)
    fn is_retryable_error(error: &net::http::Error) -> bool {
        error.is_connect() || error.is_timeout()
    }

    /// Whether a request that answered `status` may be sent again. A read
    /// may, on any transient status. A write only on a status that refused it
    /// before it ran (429, 503 without the outcome header): after a 500, 502
    /// or 504, or a 503 the node marked indeterminate at shutdown, it may have
    /// committed, a 408 leaves it running on the server, and a resend could
    /// apply it twice or hide its outcome (`defra.txn.commit_outcome.no_resend`).
    pub fn may_resend(method: &str, status: StatusCode) -> bool {
        match method {
            "GET" => Self::is_retryable_status(status),
            _ => matches!(status.as_u16(), 429 | 503),
        }
    }

    /// The error a request that got no answer ends in: a write that reached
    /// the server may have committed (`defra.txn.commit_outcome.indeterminate`).
    fn send_error(method: &str, error: net::http::Error) -> Error {
        match method == "GET" || error.is_connect() {
            true => Error::HttpRequest(error),
            false => Error::Unanswered(error),
        }
    }

    /// Whether a request that failed with `error` may be sent again: a read
    /// on a connect error or a timeout, a write only when it never connected.
    fn may_resend_after(method: &str, error: &net::http::Error) -> bool {
        match method {
            "GET" => Self::is_retryable_error(error),
            _ => error.is_connect(),
        }
    }

    /// Execute a request with retry logic for transient failures
    async fn send_with_retry(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
    ) -> Result<Response> {
        self.send_with_retries(method, url, body, MAX_RETRIES).await
    }

    /// Execute a request, retrying a transient failure at most `max_retries`
    /// times.
    async fn send_with_retries(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        max_retries: u32,
    ) -> Result<Response> {
        let mut last_error = None;
        let mut backoff_ms = INITIAL_BACKOFF_MS;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                // Always log retries so users know about transient failures
                eprintln!(
                    "Warning: Request failed, retry attempt {}/{} after {}ms delay...",
                    attempt, max_retries, backoff_ms
                );
                nuclei::time::sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms *= 2; // Exponential backoff
            }

            if self.verbose {
                eprintln!(">>> {} {}", method, url);
                if let Some(b) = body {
                    eprintln!(">>> Body: {}", b);
                }
            }

            let request = match method {
                "GET" => self.add_auth_header(self.client.get(url)),
                "POST" => {
                    let req = self.add_auth_header(self.client.post(url));
                    if let Some(b) = body {
                        req.header("Content-Type", "application/json")
                            .body(b.to_string())
                    } else {
                        req
                    }
                }
                "PATCH" => {
                    let req = self.add_auth_header(self.client.patch(url));
                    if let Some(b) = body {
                        req.header("Content-Type", "application/json")
                            .body(b.to_string())
                    } else {
                        req
                    }
                }
                "DELETE" => {
                    let req = self.add_auth_header(self.client.delete(url));
                    if let Some(b) = body {
                        req.header("Content-Type", "application/json")
                            .body(b.to_string())
                    } else {
                        req
                    }
                }
                _ => {
                    return Err(Error::Server(format!(
                        "Unsupported HTTP method: {}",
                        method
                    )))
                }
            };

            match request.send().await {
                Ok(response) => {
                    if self.verbose {
                        eprintln!("<<< HTTP {}", response.status());
                    }

                    // Don't retry successful responses or client errors (4xx except specific ones)
                    if response.status().is_success()
                        || (response.status().is_client_error()
                            && !Self::may_resend(method, response.status()))
                    {
                        return Ok(response);
                    }

                    // Check if this status is retryable
                    let marked = response.headers().contains_key(defra_core::OUTCOME_HEADER);
                    if Self::may_resend(method, response.status())
                        && !marked
                        && attempt < max_retries
                    {
                        if self.verbose {
                            eprintln!("Retryable status code: {}", response.status());
                        }
                        last_error = Some(Error::Server(format!("HTTP {}", response.status())));
                        continue;
                    }

                    return Ok(response);
                }
                Err(e) => {
                    if self.verbose {
                        eprintln!("<<< Error: {}", e);
                    }

                    // Only retry on transient errors
                    if Self::may_resend_after(method, &e) && attempt < max_retries {
                        if self.verbose {
                            eprintln!("Retryable error, will retry...");
                        }
                        last_error = Some(Error::HttpRequest(e));
                        continue;
                    }

                    return Err(Self::send_error(method, e));
                }
            }
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| {
            Error::Server(format!(
                "{} {} failed after {} retries",
                method, url, max_retries
            ))
        }))
    }

    /// Extract error details from a failed response and return an error: the
    /// server's `{"error": …}` text unchanged, as Go's client returns it, or
    /// the status and raw body when the body is not that shape. A write
    /// answered 408 or a 5xx other than 503 may have committed, so it is
    /// `indeterminate` unless the body says what the commit did
    /// (`defra.txn.commit_outcome.indeterminate`).
    async fn extract_error(method: &str, response: Response) -> Error {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|e| format!("[failed to read body: {}]", e));
        let error = match serde_json::from_str::<types::ErrorResponse>(&body) {
            Ok(parsed) => parsed.into_error(),
            Err(_) => Error::Server(format!("HTTP {}: {}", status, body.trim())),
        };
        match method != "GET" && Self::may_follow_a_commit(status) {
            true => error.unless_outcome_known(),
            false => error,
        }
    }

    /// Whether a write answered `status` may have committed: the server
    /// failed after it took the write (a 5xx other than 503's refusal), or
    /// stopped waiting for it while it ran on (408).
    fn may_follow_a_commit(status: StatusCode) -> bool {
        status == StatusCode::REQUEST_TIMEOUT
            || (status.is_server_error() && status != StatusCode::SERVICE_UNAVAILABLE)
    }

    /// Helper for POST requests with plain text body
    async fn post_text(&self, url: &str, text: &str, txn_id: Option<&str>) -> Result<Response> {
        let mut request = self
            .add_auth_header(self.client.post(url))
            .header("Content-Type", "text/plain")
            .body(text.to_string());
        if let Some(txn_id) = txn_id {
            request = request.header("x-defradb-tx", txn_id);
        }

        if self.verbose {
            eprintln!(">>> POST {}", url);
            eprintln!(">>> Body: {}", text);
        }

        let response = request
            .send()
            .await
            .map_err(|error| Self::send_error("POST", error))?;

        if self.verbose {
            eprintln!("<<< HTTP {}", response.status());
        }

        Ok(response)
    }

    /// Helper for POST requests with JSON body
    async fn post_json<T: DeserializeOwned>(&self, url: &str, body: &impl Serialize) -> Result<T> {
        let body_str = serde_json::to_string(body)?;
        let response = self.send_with_retry("POST", url, Some(&body_str)).await?;

        if !response.status().is_success() {
            return Err(Self::extract_error("POST", response).await);
        }

        let result: T = response.json().await?;
        Ok(result)
    }

    /// Helper for POST requests with a pre-serialized JSON body and optional txn header.
    async fn post_json_text(
        &self,
        url: &str,
        text: &str,
        txn_id: Option<&str>,
    ) -> Result<Response> {
        let mut request = self
            .add_auth_header(self.client.post(url))
            .header("Content-Type", "application/json")
            .body(text.to_string());
        if let Some(txn_id) = txn_id {
            request = request.header("x-defradb-tx", txn_id);
        }

        if self.verbose {
            eprintln!(">>> POST {}", url);
            eprintln!(">>> Body: {}", text);
        }

        let response = request
            .send()
            .await
            .map_err(|error| Self::send_error("POST", error))?;

        if self.verbose {
            eprintln!("<<< HTTP {}", response.status());
        }

        Ok(response)
    }

    async fn request_json<T: DeserializeOwned>(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
    ) -> Result<T> {
        let response = self.send_with_retry(method, url, body).await?;
        if !response.status().is_success() {
            return Err(Self::extract_error(method, response).await);
        }
        Ok(response.json().await?)
    }

    pub(crate) async fn request_void(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
    ) -> Result<()> {
        let response = self.send_with_retry(method, url, body).await?;
        if !response.status().is_success() {
            return Err(Self::extract_error(method, response).await);
        }
        Ok(())
    }

    /// POST `path` with no body: its JSON answer, or null when it has none.
    async fn post_json_or_null(&self, path: &str) -> Result<JsonValue> {
        let url = format!("{}{}", self.base_url, path);
        let text = self.request_text("POST", &url, None).await?;
        match text.trim().is_empty() {
            true => Ok(JsonValue::Null),
            false => Ok(serde_json::from_str(&text)?),
        }
    }

    async fn request_text(&self, method: &str, url: &str, body: Option<&str>) -> Result<String> {
        let response = self.send_with_retry(method, url, body).await?;
        if !response.status().is_success() {
            return Err(Self::extract_error(method, response).await);
        }
        Ok(response.text().await?)
    }

    endpoint!(action_list, GET "/api/v0/actions" => Vec<defra_core::ActionExecution>);

    // P2P
    endpoint!(collection_versions, GET  "/api/v0/collections?get_inactive=true" => Vec<schema::CollectionVersion>);
    // ACP / NAC
    endpoint!(acp_list_policies,   GET  "/api/v0/acp/policy"         => Vec<AcpPolicy>);
    endpoint!(nac_status,          GET  "/api/v0/acp/node/status"    => JsonValue);
    /// Disable node access control. Go answers with an empty body, read as null.
    pub async fn nac_disable(&self) -> Result<JsonValue> {
        self.post_json_or_null("/api/v0/acp/node/disable").await
    }

    /// Re-enable node access control. Go answers with an empty body, read as null.
    pub async fn nac_re_enable(&self) -> Result<JsonValue> {
        self.post_json_or_null("/api/v0/acp/node/re-enable").await
    }

    // Lens

    // Node
    endpoint!(node_identity,       GET  "/api/v0/node/identity"      => JsonValue);
    endpoint!(purge,               POST "/api/v0/purge");
}

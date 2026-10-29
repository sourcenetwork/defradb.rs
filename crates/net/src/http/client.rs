//! The HTTP client and its builder.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use edge::Edge;
use http::header::{
    AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, LOCATION, PROXY_AUTHORIZATION,
    TRANSFER_ENCODING, WWW_AUTHENTICATE,
};
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper_util::client::legacy;
use hyper_util::client::proxy::matcher::Matcher;
use nuclei::time::Instant;

use super::request::{Parts, RequestBuilder};
use super::response::{within, Response};
use super::url::IntoUrl;
use super::Error;
use crate::connect::Connector;
use crate::rt::{Executor, Timer};
use crate::tls::{Certificate, ClientTls, TlsConnector, Version};

/// How many redirects a request follows before it fails.
const REDIRECT_LIMIT: usize = 10;

/// An HTTP client: a pool of connections on nuclei sockets, each driven by
/// a task of the network edge it was built on, HTTP/2 where the server
/// negotiates it, TLS with the system's root certificates, the system's
/// proxies, and redirects followed. Cloning shares the pool.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    hyper: legacy::Client<Connector, Full<Bytes>>,
    timeout: Option<Duration>,
    default_headers: HeaderMap,
    proxies: Option<Arc<Matcher>>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("timeout", &self.inner.timeout)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// A client on `edge` with every other setting at its default. If the
    /// TLS configuration cannot be built, the client still serves plain HTTP
    /// and every HTTPS request fails with the reason.
    pub fn new(edge: Edge) -> Self {
        let builder = ClientBuilder::new(edge);
        let tls = builder
            .tls
            .config()
            .map_err(|error| Arc::from(error.to_string()));
        builder.assemble(tls)
    }

    /// A builder of a client on `edge`.
    pub fn builder(edge: Edge) -> ClientBuilder {
        ClientBuilder::new(edge)
    }

    pub fn get(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::GET, url)
    }

    pub fn post(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::POST, url)
    }

    pub fn put(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::PUT, url)
    }

    pub fn patch(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::PATCH, url)
    }

    pub fn delete(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::DELETE, url)
    }

    pub fn head(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::HEAD, url)
    }

    pub fn request(&self, method: Method, url: impl IntoUrl) -> RequestBuilder {
        RequestBuilder::new(
            self.clone(),
            url.into_url().map(|url| Parts::new(method, url)),
        )
    }

    /// Sends `parts` with the client's default headers, following
    /// redirects, all within the request's time limit (or the client's).
    pub(crate) async fn execute(&self, mut parts: Parts) -> Result<Response, Error> {
        for (name, value) in &self.inner.default_headers {
            if !parts.headers.contains_key(name) {
                parts.headers.insert(name.clone(), value.clone());
            }
        }
        let deadline = parts
            .timeout
            .or(self.inner.timeout)
            .map(|limit| Instant::now() + limit);
        let url = parts.url.clone();
        within(deadline, &url, self.follow(parts, deadline)).await?
    }

    async fn follow(&self, mut parts: Parts, deadline: Option<Instant>) -> Result<Response, Error> {
        for _ in 0..=REDIRECT_LIMIT {
            let response = self.exchange(&parts).await?;
            match redirect(&parts, &response)? {
                Some(next) => parts = next,
                None => return Ok(Response::new(response, parts.url, deadline)),
            }
        }
        Err(Error::TooManyRedirects {
            url: parts.url.to_string(),
            limit: REDIRECT_LIMIT,
        })
    }

    async fn exchange(&self, parts: &Parts) -> Result<http::Response<Incoming>, Error> {
        let uri: Uri = parts.url.as_str().parse().map_err(Error::builder)?;
        let mut headers = parts.headers.clone();
        if parts.url.scheme() == "http" && !headers.contains_key(PROXY_AUTHORIZATION) {
            let auth = self
                .inner
                .proxies
                .as_ref()
                .and_then(|proxies| proxies.intercept(&uri))
                .and_then(|intercept| intercept.basic_auth().cloned());
            if let Some(auth) = auth {
                headers.insert(PROXY_AUTHORIZATION, auth);
            }
        }
        let mut request = http::Request::builder()
            .method(parts.method.clone())
            .uri(uri)
            .body(Full::new(parts.body.clone()))
            .map_err(Error::builder)?;
        *request.headers_mut() = headers;
        self.inner
            .hyper
            .request(request)
            .await
            .map_err(|error| Error::from_client(&parts.url, error))
    }
}

/// The request a redirect asks for, or `None` when `response` is not one.
/// 301 and 302 turn a POST into a GET, 303 turns anything but a HEAD into
/// a GET, 307 and 308 keep the method and body; credentials are not sent to
/// another origin.
fn redirect(parts: &Parts, response: &http::Response<Incoming>) -> Result<Option<Parts>, Error> {
    let status = response.status();
    if !status.is_redirection() || status == StatusCode::NOT_MODIFIED {
        return Ok(None);
    }
    let Some(location) = response.headers().get(LOCATION) else {
        return Ok(None);
    };
    let location = location.to_str().map_err(Error::builder)?;
    let url = parts.url.join(location).map_err(Error::builder)?;
    let mut next = parts.clone();
    let to_get = match status {
        StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND => parts.method == Method::POST,
        StatusCode::SEE_OTHER => parts.method != Method::HEAD,
        StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT => false,
        _ => return Ok(None),
    };
    if to_get {
        next.method = Method::GET;
        next.body = Bytes::new();
        for header in [CONTENT_TYPE, CONTENT_LENGTH, TRANSFER_ENCODING] {
            next.headers.remove(header);
        }
    }
    if url.origin() != parts.url.origin() {
        for header in [AUTHORIZATION, COOKIE, PROXY_AUTHORIZATION, WWW_AUTHENTICATE] {
            next.headers.remove(header);
        }
    }
    next.url = url;
    Ok(Some(next))
}

/// Builds a [`Client`].
#[derive(Debug, Clone)]
pub struct ClientBuilder {
    edge: Edge,
    timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    pool_idle_timeout: Option<Duration>,
    pool_max_idle_per_host: usize,
    proxies: bool,
    tls: ClientTls,
    default_headers: HeaderMap,
}

impl ClientBuilder {
    fn new(edge: Edge) -> Self {
        Self {
            edge,
            timeout: None,
            connect_timeout: None,
            pool_idle_timeout: Some(Duration::from_secs(90)),
            pool_max_idle_per_host: usize::MAX,
            proxies: true,
            tls: ClientTls::default(),
            default_headers: HeaderMap::new(),
        }
    }

    /// The time limit of every request, from connecting to the end of the
    /// response body, unless the request sets its own.
    pub fn timeout(mut self, limit: Duration) -> Self {
        self.timeout = Some(limit);
        self
    }

    pub fn connect_timeout(mut self, limit: Duration) -> Self {
        self.connect_timeout = Some(limit);
        self
    }

    pub fn pool_max_idle_per_host(mut self, max: usize) -> Self {
        self.pool_max_idle_per_host = max;
        self
    }

    /// Only HTTP/1.1, even where the server offers HTTP/2.
    pub fn http1_only(mut self) -> Self {
        self.tls.alpn = vec![b"http/1.1".to_vec()];
        self
    }

    /// Ignores the system's proxies.
    pub fn no_proxy(mut self) -> Self {
        self.proxies = false;
        self
    }

    pub fn add_root_certificate(mut self, certificate: Certificate) -> Self {
        self.tls.roots.push(certificate);
        self
    }

    /// Whether the system's root certificates are trusted (default true).
    pub fn tls_built_in_root_certs(mut self, trusted: bool) -> Self {
        self.tls.system_roots = trusted;
        self
    }

    pub fn min_tls_version(mut self, version: Version) -> Self {
        self.tls.min = Some(version);
        self
    }

    pub fn max_tls_version(mut self, version: Version) -> Self {
        self.tls.max = Some(version);
        self
    }

    /// Headers every request carries unless it sets them itself.
    pub fn default_headers(mut self, headers: HeaderMap) -> Self {
        self.default_headers = headers;
        self
    }

    pub fn build(self) -> Result<Client, Error> {
        let tls = self.tls.config().map_err(Error::builder)?;
        Ok(self.assemble(Ok(tls)))
    }

    fn assemble(self, tls: Result<Arc<rustls::ClientConfig>, Arc<str>>) -> Client {
        let proxies = self.proxies.then(|| Arc::new(Matcher::from_system()));
        let connector = Connector {
            tls: tls.map(TlsConnector::from),
            connect_timeout: self.connect_timeout,
            proxies: proxies.clone(),
        };
        let hyper = legacy::Client::builder(Executor::new(self.edge))
            .timer(Timer)
            .pool_timer(Timer)
            .pool_idle_timeout(self.pool_idle_timeout)
            .pool_max_idle_per_host(self.pool_max_idle_per_host)
            .build(connector);
        Client {
            inner: Arc::new(Inner {
                hyper,
                timeout: self.timeout,
                default_headers: self.default_headers,
                proxies,
            }),
        }
    }
}

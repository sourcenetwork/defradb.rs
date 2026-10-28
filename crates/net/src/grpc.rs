//! The HTTP/2 channel tonic's generated clients run on, over nuclei
//! sockets: `Client::new(channel)` with a [`Channel`] from an [`Endpoint`].

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use http::uri::{Authority, PathAndQuery, Scheme};
use http::{Request, Response, Uri};
use hyper::body::Incoming;
use hyper_util::client::legacy;
use tower_service::Service;

use crate::connect::Connector;
use crate::rt::{Executor, Timer};
use crate::tls::{ClientTls, TlsConnector};
use crate::BoxError;

/// Why a channel could not be made, or a call on it failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid gRPC endpoint: {0}")]
    Endpoint(String),
    #[error("cannot connect to {endpoint}: {source}")]
    Connect {
        endpoint: String,
        #[source]
        source: BoxError,
    },
    #[error("the gRPC call to {endpoint} timed out")]
    Timeout { endpoint: String },
    #[error("the gRPC call to {endpoint} failed: {source}")]
    Call {
        endpoint: String,
        #[source]
        source: BoxError,
    },
}

/// Where a channel connects, and how.
#[derive(Debug, Clone)]
pub struct Endpoint {
    uri: Uri,
    connect_timeout: Option<Duration>,
    timeout: Option<Duration>,
    tls: ClientTls,
}

impl Endpoint {
    /// An endpoint at `uri`: `http://` speaks HTTP/2 in the clear, `https://`
    /// over TLS with the system's root certificates.
    pub fn from_shared(uri: impl Into<String>) -> Result<Self, Error> {
        let uri: Uri = uri
            .into()
            .parse()
            .map_err(|error: http::uri::InvalidUri| Error::Endpoint(error.to_string()))?;
        if uri.scheme().is_none() || uri.authority().is_none() {
            return Err(Error::Endpoint(format!(
                "{uri}: a scheme and a host are needed"
            )));
        }
        Ok(Self {
            uri,
            connect_timeout: None,
            timeout: None,
            tls: ClientTls {
                alpn: vec![b"h2".to_vec()],
                ..ClientTls::default()
            },
        })
    }

    pub fn connect_timeout(mut self, limit: Duration) -> Self {
        self.connect_timeout = Some(limit);
        self
    }

    /// The time limit of every call.
    pub fn timeout(mut self, limit: Duration) -> Self {
        self.timeout = Some(limit);
        self
    }

    /// A channel whose first connection is made now, so an unreachable
    /// endpoint fails here.
    pub async fn connect<B>(self) -> Result<Channel<B>, Error>
    where
        B: http_body::Body + Send + Unpin + 'static,
        B::Data: Send,
        B::Error: Into<BoxError>,
    {
        let channel = self.connect_lazy()?;
        let mut connector = channel.connector.clone();
        connector
            .call(channel.inner.uri.clone())
            .await
            .map_err(|source| Error::Connect {
                endpoint: channel.inner.uri.to_string(),
                source,
            })?;
        Ok(channel)
    }

    /// A channel that connects on its first call.
    pub fn connect_lazy<B>(self) -> Result<Channel<B>, Error>
    where
        B: http_body::Body + Send + Unpin + 'static,
        B::Data: Send,
        B::Error: Into<BoxError>,
    {
        let tls = self
            .tls
            .config()
            .map_err(|error| Error::Endpoint(error.to_string()))?;
        let connector = Connector {
            tls: Ok(TlsConnector::from(tls)),
            connect_timeout: self.connect_timeout,
            proxies: None,
        };
        let client = legacy::Client::builder(Executor)
            .timer(Timer)
            .pool_timer(Timer)
            .http2_only(true)
            .build(connector.clone());
        let scheme = self.uri.scheme().cloned().unwrap_or(Scheme::HTTP);
        let authority = self
            .uri
            .authority()
            .cloned()
            .ok_or_else(|| Error::Endpoint(format!("{}: a host is needed", self.uri)))?;
        Ok(Channel {
            client,
            connector,
            inner: Arc::new(Origin {
                uri: self.uri,
                scheme,
                authority,
                timeout: self.timeout,
            }),
        })
    }
}

/// An HTTP/2 channel to one endpoint; cloning shares its connections.
pub struct Channel<B> {
    client: legacy::Client<Connector, B>,
    connector: Connector,
    inner: Arc<Origin>,
}

struct Origin {
    uri: Uri,
    scheme: Scheme,
    authority: Authority,
    timeout: Option<Duration>,
}

impl<B> Clone for Channel<B> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            connector: self.connector.clone(),
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<B> Service<Request<B>> for Channel<B>
where
    B: http_body::Body + Send + Unpin + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    type Response = Response<Incoming>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response<Incoming>, Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let client = self.client.clone();
        let origin = Arc::clone(&self.inner);
        Box::pin(async move {
            let endpoint = origin.uri.to_string();
            let (mut parts, body) = request.into_parts();
            let path = parts
                .uri
                .path_and_query()
                .cloned()
                .unwrap_or_else(|| PathAndQuery::from_static("/"));
            parts.uri = Uri::builder()
                .scheme(origin.scheme.clone())
                .authority(origin.authority.clone())
                .path_and_query(path)
                .build()
                .map_err(|error| Error::Endpoint(error.to_string()))?;
            let calling = client.request(Request::from_parts(parts, body));
            let answered = match origin.timeout {
                Some(limit) => {
                    nuclei::time::timeout(limit, calling)
                        .await
                        .map_err(|_| Error::Timeout {
                            endpoint: endpoint.clone(),
                        })?
                }
                None => calling.await,
            };
            answered.map_err(|error| Error::Call {
                endpoint,
                source: Box::new(error),
            })
        })
    }
}

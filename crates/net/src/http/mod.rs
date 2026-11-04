//! The HTTP client: [`Client`], its [`ClientBuilder`], the
//! [`RequestBuilder`] of one request, and its [`Response`].
//!
//! Connections are nuclei sockets, pooled per host; HTTP/2 is used where
//! the server offers it by ALPN. TLS trusts the system's root certificates,
//! a proxy the system configures (environment variables, and the system's
//! settings on macOS and Windows) is used where it applies, and up to ten
//! redirects are followed.

mod client;
mod error;
mod request;
mod response;
mod url;

pub use client::{Client, ClientBuilder};
pub use error::Error;
pub use http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
pub use request::RequestBuilder;
pub use response::{BytesStream, Response};
pub use url::IntoUrl;

/// The TLS settings a [`ClientBuilder`] takes.
pub mod tls {
    pub use crate::tls::{Certificate, TlsError, Version};
}

/// Sends a GET to `url` with a client of default settings on `edge`.
pub async fn get(url: impl IntoUrl, edge: edge::Edge) -> Result<Response, Error> {
    Client::new(edge).get(url).send().await
}

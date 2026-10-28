//! Why an HTTP request has no answer, or an answer that is an error.

use http::StatusCode;

use crate::BoxError;

/// Why an HTTP request failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The request or the client could not be built.
    #[error("invalid HTTP request: {0}")]
    Builder(String),
    /// No connection to the server could be made.
    #[error("cannot connect to {url}: {source}")]
    Connect {
        url: String,
        #[source]
        source: BoxError,
    },
    /// The request, or its connection, ran past its time limit.
    #[error("the request to {url} timed out")]
    Timeout { url: String, connecting: bool },
    /// The request failed after a connection was made.
    #[error("the request to {url} failed: {source}")]
    Request {
        url: String,
        #[source]
        source: BoxError,
    },
    /// The server answered with an error status ([`Response::error_for_status`](super::Response::error_for_status)).
    #[error("{url} answered {status}")]
    Status { url: String, status: StatusCode },
    /// Reading the response body failed.
    #[error("reading the response from {url} failed: {source}")]
    Body {
        url: String,
        #[source]
        source: BoxError,
    },
    /// The response body is not what was asked for.
    #[error("the response from {url} cannot be decoded: {source}")]
    Decode {
        url: String,
        #[source]
        source: serde_json::Error,
    },
    /// Redirects went on past the limit.
    #[error("{url} redirected more than {limit} times")]
    TooManyRedirects { url: String, limit: usize },
}

impl Error {
    pub(crate) fn builder(reason: impl std::fmt::Display) -> Self {
        Self::Builder(reason.to_string())
    }

    /// A failure of the pooled client, classified by what failed: making a
    /// connection (and whether that ran out of time), or the exchange.
    pub(crate) fn from_client(url: &url::Url, error: hyper_util::client::legacy::Error) -> Self {
        let url = url.to_string();
        if !error.is_connect() {
            return Self::Request {
                url,
                source: Box::new(error),
            };
        }
        if timed_out(&error) {
            return Self::Timeout {
                url,
                connecting: true,
            };
        }
        Self::Connect {
            url,
            source: Box::new(error),
        }
    }

    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }

    pub fn is_connect(&self) -> bool {
        matches!(
            self,
            Self::Connect { .. }
                | Self::Timeout {
                    connecting: true,
                    ..
                }
        )
    }

    pub fn is_status(&self) -> bool {
        matches!(self, Self::Status { .. })
    }

    pub fn is_builder(&self) -> bool {
        matches!(self, Self::Builder(_))
    }

    pub fn is_body(&self) -> bool {
        matches!(self, Self::Body { .. })
    }

    pub fn is_decode(&self) -> bool {
        matches!(self, Self::Decode { .. })
    }

    pub fn is_redirect(&self) -> bool {
        matches!(self, Self::TooManyRedirects { .. })
    }

    /// The status the server answered, when that is the failure.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Status { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// Whether a connection failure was the connect timeout running out.
fn timed_out(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut cause = error.source();
    while let Some(error) = cause {
        if error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::TimedOut)
        {
            return true;
        }
        cause = error.source();
    }
    false
}

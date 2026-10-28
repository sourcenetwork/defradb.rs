//! A response, and its body read within the request's time limit.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use http::header::CONTENT_LENGTH;
use http::{HeaderMap, StatusCode, Version};
use http_body::Body as _;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use nuclei::time::{Instant, Sleep};
use serde::de::DeserializeOwned;
use url::Url;

use super::Error;

/// An HTTP response. The body is read once, by one of the reading methods.
#[derive(Debug)]
pub struct Response {
    status: StatusCode,
    version: Version,
    headers: HeaderMap,
    url: Url,
    body: Incoming,
    deadline: Option<Instant>,
}

impl Response {
    pub(crate) fn new(
        response: http::Response<Incoming>,
        url: Url,
        deadline: Option<Instant>,
    ) -> Self {
        let (parts, body) = response.into_parts();
        Self {
            status: parts.status,
            version: parts.version,
            headers: parts.headers,
            url,
            body,
            deadline,
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The URL the response came from, after any redirect.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// The body's length, when the server said it.
    pub fn content_length(&self) -> Option<u64> {
        self.body.size_hint().exact().or_else(|| {
            self.headers
                .get(CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok())
        })
    }

    /// This response, or [`Error::Status`] when its status is a client or
    /// server error.
    pub fn error_for_status(self) -> Result<Self, Error> {
        self.status_error().map_or(Ok(self), Err)
    }

    pub fn error_for_status_ref(&self) -> Result<&Self, Error> {
        self.status_error().map_or(Ok(self), Err)
    }

    fn status_error(&self) -> Option<Error> {
        (self.status.is_client_error() || self.status.is_server_error()).then(|| Error::Status {
            url: self.url.to_string(),
            status: self.status,
        })
    }

    pub async fn bytes(self) -> Result<Bytes, Error> {
        let url = self.url;
        let collected = within(self.deadline, &url, self.body.collect()).await?;
        collected
            .map(|body| body.to_bytes())
            .map_err(|error| body_error(&url, error))
    }

    /// The body as text; bytes that are not UTF-8 become U+FFFD.
    pub async fn text(self) -> Result<String, Error> {
        let bytes = self.bytes().await?;
        Ok(match String::from_utf8(bytes.into()) {
            Ok(text) => text,
            Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
        })
    }

    pub async fn json<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let url = self.url.clone();
        let bytes = self.bytes().await?;
        serde_json::from_slice(&bytes).map_err(|source| Error::Decode {
            url: url.to_string(),
            source,
        })
    }

    /// The body's next piece, or `None` at its end.
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, Error> {
        let deadline = self.deadline;
        let url = self.url.clone();
        within(deadline, &url, next_data(&mut self.body))
            .await?
            .map_err(|error| body_error(&url, error))
    }

    /// The body as a stream of pieces, each read within the request's time
    /// limit.
    pub fn bytes_stream(self) -> BytesStream {
        BytesStream {
            body: self.body,
            sleep: self
                .deadline
                .map(|at| Box::pin(nuclei::time::sleep_until(at))),
            url: self.url,
        }
    }
}

/// The pieces of a response body. See [`Response::bytes_stream`].
pub struct BytesStream {
    body: Incoming,
    sleep: Option<Pin<Box<Sleep>>>,
    url: Url,
}

impl Stream for BytesStream {
    type Item = Result<Bytes, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            match Pin::new(&mut this.body).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => {
                    if let Ok(data) = frame.into_data() {
                        return Poll::Ready(Some(Ok(data)));
                    }
                }
                Poll::Ready(Some(Err(error))) => {
                    return Poll::Ready(Some(Err(body_error(&this.url, error))))
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => {
                    if let Some(sleep) = &mut this.sleep {
                        if sleep.as_mut().poll(cx).is_ready() {
                            this.sleep = None;
                            return Poll::Ready(Some(Err(Error::Timeout {
                                url: this.url.to_string(),
                                connecting: false,
                            })));
                        }
                    }
                    return Poll::Pending;
                }
            }
        }
    }
}

async fn next_data(body: &mut Incoming) -> Result<Option<Bytes>, hyper::Error> {
    while let Some(frame) = body.frame().await {
        if let Ok(data) = frame?.into_data() {
            return Ok(Some(data));
        }
    }
    Ok(None)
}

fn body_error(url: &Url, error: hyper::Error) -> Error {
    Error::Body {
        url: url.to_string(),
        source: Box::new(error),
    }
}

/// `work`'s output, or [`Error::Timeout`] once `deadline` passes first.
pub(crate) async fn within<F>(
    deadline: Option<Instant>,
    url: &Url,
    work: F,
) -> Result<F::Output, Error>
where
    F: Future,
{
    match deadline {
        Some(at) => nuclei::time::timeout_at(at, work)
            .await
            .map_err(|_| Error::Timeout {
                url: url.to_string(),
                connecting: false,
            }),
        None => Ok(work.await),
    }
}

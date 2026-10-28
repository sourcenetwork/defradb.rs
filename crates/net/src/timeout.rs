//! A time limit on each request a tower service handles, on nuclei's
//! clock: tower's own `TimeoutLayer` arms a tokio timer, which panics on a
//! nuclei runtime.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use nuclei::time::Sleep;
use tower_layer::Layer;
use tower_service::Service;

use crate::BoxError;

/// The request ran past its time limit.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("the request ran past its time limit")]
pub struct Elapsed;

/// Fails each request still running after `limit` with [`Elapsed`].
#[derive(Debug, Clone, Copy)]
pub struct TimeoutLayer {
    limit: Duration,
}

impl TimeoutLayer {
    pub fn new(limit: Duration) -> Self {
        Self { limit }
    }
}

impl<S> Layer<S> for TimeoutLayer {
    type Service = Timeout<S>;

    fn layer(&self, inner: S) -> Timeout<S> {
        Timeout {
            inner,
            limit: self.limit,
        }
    }
}

/// `inner`, each request within a time limit. See [`TimeoutLayer`].
#[derive(Debug, Clone)]
pub struct Timeout<S> {
    inner: S,
    limit: Duration,
}

impl<S, R> Service<R> for Timeout<S>
where
    S: Service<R>,
    S::Error: Into<BoxError>,
{
    type Response = S::Response;
    type Error = BoxError;
    type Future = Timed<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), BoxError>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: R) -> Self::Future {
        Timed {
            inner: self.inner.call(request),
            sleep: nuclei::time::sleep(self.limit),
        }
    }
}

pin_project_lite::pin_project! {
    /// A response on its way, or [`Elapsed`] once the limit passes first.
    pub struct Timed<F> {
        #[pin]
        inner: F,
        #[pin]
        sleep: Sleep,
    }
}

impl<F, T, E> Future for Timed<F>
where
    F: Future<Output = Result<T, E>>,
    E: Into<BoxError>,
{
    type Output = Result<T, BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if let Poll::Ready(result) = this.inner.poll(cx) {
            return Poll::Ready(result.map_err(Into::into));
        }
        this.sleep
            .poll(cx)
            .map(|()| Err(Box::new(Elapsed) as BoxError))
    }
}

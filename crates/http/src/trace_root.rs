//! Per-request trace root.
//!
//! `fastrace` keeps its parent in a thread-local and **discards** spans that
//! have no local parent, so every `#[fastrace::trace]` below a handler would
//! be dropped without a root established here. One middleware covers every
//! route, which is also why handlers must not be instrumented before this
//! layer runs.
//!
//! `in_span` re-enters the span on each poll, so the parent survives the
//! await points inside a handler. It does **not** cross `spawn_blocking` or
//! `tokio::spawn`; those boundaries carry the context explicitly (see
//! `query_context`).

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use fastrace::future::FutureExt;
use fastrace::prelude::SpanContext;
use fastrace::Span;

pub async fn trace_root(request: Request, next: Next) -> Response {
    let name = format!("{} {}", request.method(), request.uri().path());
    let root = Span::root(name, SpanContext::random());
    next.run(request).in_span(root).await
}

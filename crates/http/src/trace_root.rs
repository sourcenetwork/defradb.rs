//! Per-request trace root.
//!
//! `fastrace` keeps its parent in a thread-local and **discards** spans that
//! have no local parent, so every `#[fastrace::trace]` below a handler would
//! be dropped without a root established here.
//!
//! The span is named after the matched *route template* rather than the
//! request URI. Concrete paths embed document IDs and transaction IDs
//! (`/collections/Item/document/bae-1f66…`), which would make span names
//! unbounded in cardinality — the route template keeps them to one per route,
//! and matches OpenTelemetry's `{method} {http.route}` convention.
//!
//! Installed with `route_layer` so routing has already run and `MatchedPath`
//! is populated; that also means unmatched requests (404) start no trace.
//!
//! `in_span` re-enters the span on each poll, so the parent survives await
//! points inside a handler. It does **not** cross `spawn_blocking` or
//! `tokio::spawn`; those boundaries carry the context explicitly (see
//! `query_context`).

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use fastrace::future::FutureExt;
use fastrace::prelude::SpanContext;
use fastrace::Span;

pub async fn trace_root(
    matched_path: Option<MatchedPath>,
    request: Request,
    next: Next,
) -> Response {
    let route = matched_path
        .as_ref()
        .map(|mp| mp.as_str())
        .unwrap_or("<unmatched>");
    let name = format!("{} {}", request.method(), route);
    let root = Span::root(name, SpanContext::random());
    next.run(request).in_span(root).await
}

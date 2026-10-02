//! Per-request trace root.
//!
//! `fastrace` keeps its parent in a thread-local and **discards** spans that
//! have no local parent, so every `#[fastrace::trace]` below a handler would be
//! dropped without a root established here.
//!
//! An inbound W3C `traceparent` is honoured so a caller's distributed trace
//! continues through this node instead of being severed at the boundary. The
//! header also carries the sampling decision in its flags, which is what makes
//! a sampled trace arrive whole: the choice is made once at the origin root and
//! inherited, never re-rolled per span.
//!
//! The span is named after the matched *route template* rather than the request
//! URI. Concrete paths embed document and transaction IDs
//! (`/collections/Item/document/bae-1f66…`), which would make span names
//! unbounded in cardinality; the template keeps them to one per route and
//! matches OpenTelemetry's `{method} {http.route}` convention.
//!
//! Installed with `route_layer` so routing has already run and `MatchedPath` is
//! populated; that also means unmatched requests (404) start no trace.
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

/// W3C Trace Context header carrying the caller's trace id, parent span id and
/// sampling decision.
const TRACEPARENT: &str = "traceparent";

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

    // Continue the caller's trace when it sent one; a malformed header falls
    // back to a fresh trace rather than dropping the request's spans.
    let parent = request
        .headers()
        .get(TRACEPARENT)
        .and_then(|value| value.to_str().ok())
        .and_then(SpanContext::decode_w3c_traceparent)
        .unwrap_or_else(SpanContext::random);

    let root = Span::root(name, parent);
    next.run(request).in_span(root).await
}

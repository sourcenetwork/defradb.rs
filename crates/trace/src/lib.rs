//! Span instrumentation over `fastrace`, supplying the two things it leaves to
//! the application.
//!
//! **Per-callsite filtering.** `fastrace` has none: `Config` exposes only
//! `report_interval`, and `tail_sampled` / `max_spans_per_trace` /
//! `report_before_root_finish` are deprecated no-ops. Its sampling model is
//! whole-trace — `Span::root` versus `Span::noop`, plus `Span::cancel` — which
//! is a deliberate design choice, but it leaves no way to leave a hot span
//! compiled in and switched off, then enable it for one investigation. The
//! callsite registry here mirrors `tracing`'s: a static per site resolves once
//! against the active level, then costs a relaxed load.
//!
//! **Roots at entry points.** `fastrace` discards spans with no local parent.
//! That is documented and intentional — it lets libraries instrument without
//! requiring callers to set anything up — but it means the application must
//! establish a root wherever work begins. [`span_or_root`] makes a span
//! self-rooting, so a seam reached from HTTP continues the caller's trace while
//! the same seam reached from FFI, the embedded node, the Postgres wire
//! protocol or startup begins one.

mod callsite;

pub use callsite::{set_max_level, Callsite, Level};
pub use defra_trace_macro::traced;

#[doc(hidden)]
pub use fastrace;

/// Continue the caller's trace when there is one, otherwise start a new one.
///
/// Entry points that may or may not be reached from an already-traced caller
/// want this: without it, everything below a non-HTTP entry point is silently
/// dropped.
#[inline]
pub fn span_or_root(name: &'static str) -> fastrace::Span {
    match fastrace::collector::SpanContext::current_local_parent() {
        Some(_) => fastrace::Span::enter_with_local_parent(name),
        None => fastrace::Span::root(name, fastrace::collector::SpanContext::random()),
    }
}

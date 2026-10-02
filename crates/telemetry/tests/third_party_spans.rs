//! Requires the `otlp` feature: the compat layer and this crate's
//! `tracing-subscriber` dependency are both gated behind it.
#![cfg(feature = "otlp")]

//! Dependencies (`iroh` has 84 span sites) are instrumented with `tracing`,
//! which `fastrace` cannot see. The compat layer must mirror those spans —
//! including ones created on background tasks with no local parent, which
//! `fastrace` would otherwise discard.

use std::sync::{Arc, Mutex};

use fastrace::prelude::{SpanContext, SpanRecord};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<SpanRecord>>>);

impl fastrace::collector::Reporter for Captured {
    fn report(&mut self, spans: Vec<SpanRecord>) {
        self.0.lock().expect("capture").extend(spans);
    }
}

impl Captured {
    fn take(&self) -> Vec<SpanRecord> {
        std::mem::take(&mut *self.0.lock().expect("capture"))
    }
}

/// Stands in for a dependency's instrumentation: a `tracing` span, created
/// with no knowledge of `fastrace`.
#[tracing::instrument(level = "info", name = "dependency.work", skip_all)]
fn dependency_work() {
    tracing::info!(peer = "abc123", "connected to peer");
}

#[test]
fn dependency_tracing_spans_are_mirrored_into_fastrace() {
    let cap = Captured::default();
    fastrace::set_reporter(cap.clone(), fastrace::collector::Config::default());

    // No filter layer: this crate's `tracing-subscriber` has `env-filter`
    // turned off, and the behaviour under test is the mirroring, not filtering.
    tracing_subscriber::registry()
        .with(telemetry::FastraceCompatLayer::new().boxed())
        .init();

    // 1. No fastrace parent, as on a dependency's own background task. The
    //    layer must root it rather than drop it.
    dependency_work();
    fastrace::flush();
    let spans = cap.take();
    let names: Vec<_> = spans.iter().map(|s| s.name.to_string()).collect();
    assert!(
        names.contains(&"dependency.work".to_string()),
        "an unparented dependency span was dropped: {names:?}"
    );
    let rooted = spans
        .iter()
        .find(|s| s.name.as_ref() == "dependency.work")
        .expect("span present");
    assert!(
        !rooted.events.is_empty(),
        "the dependency's event did not attach to its span"
    );

    // 2. Inside one of our traces it must join rather than start a second.
    let caller = SpanContext::random();
    {
        let root = fastrace::Span::root("ours", caller);
        let _g = root.set_local_parent();
        dependency_work();
    }
    fastrace::flush();
    let spans = cap.take();
    let ids: std::collections::HashSet<_> = spans.iter().map(|s| s.trace_id).collect();
    assert_eq!(
        ids.len(),
        1,
        "dependency span split our trace: {:?}",
        spans
            .iter()
            .map(|s| (&s.name, s.trace_id))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ids.into_iter().next().expect("one trace"),
        caller.trace_id,
        "dependency span did not join our trace"
    );
}

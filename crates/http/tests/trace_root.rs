//! A caller's trace must continue through this node, and the sampling decision
//! it made must come with it — that inheritance is what makes a sampled trace
//! arrive whole rather than in fragments.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::Request;
use defra_http::mock::MockQueryExecutor;
use defra_http::router::create_router;
use fastrace::prelude::{SpanContext, SpanId, SpanRecord, TraceId};
use tower::ServiceExt;

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

async fn get_version(traceparent: Option<&str>) {
    let mut builder = Request::builder().uri("/api/v0/version");
    if let Some(traceparent) = traceparent {
        builder = builder.header("traceparent", traceparent);
    }
    let request = builder.body(Body::empty()).expect("request should build");
    create_router(Arc::new(MockQueryExecutor::new()))
        .oneshot(request)
        .await
        .expect("router should respond");
}

#[tokio::test]
async fn inbound_traceparent_is_continued_and_absent_one_starts_a_trace() {
    let cap = Captured::default();
    fastrace::set_reporter(cap.clone(), fastrace::collector::Config::default());

    // A caller's trace id must be adopted, not replaced, or the trace is
    // severed at our boundary.
    //
    // Built explicitly rather than with `SpanContext::random()`, whose span id
    // is zero: `encode_w3c_traceparent` happily emits that, but
    // `decode_w3c_traceparent` rejects a zero span id, so a random context does
    // not survive its own round trip.
    let caller = SpanContext::new(
        TraceId(0x5f2d_1c4b_a390_7e61_8d44_2200_19ff_c3a7),
        SpanId(0x4242),
    );
    let traceparent = caller.encode_w3c_traceparent();
    get_version(Some(&traceparent)).await;
    fastrace::flush();
    let spans = cap.take();
    assert!(!spans.is_empty(), "no spans recorded for a traced request");
    assert!(
        spans.iter().all(|s| s.trace_id == caller.trace_id),
        "did not continue the caller's trace: {:?}",
        spans
            .iter()
            .map(|s| (&s.name, s.trace_id))
            .collect::<Vec<_>>()
    );

    // With no header we still trace, under a fresh id.
    get_version(None).await;
    fastrace::flush();
    let spans = cap.take();
    assert!(!spans.is_empty(), "untraced request produced no spans");
    assert!(
        spans.iter().all(|s| s.trace_id != caller.trace_id),
        "reused the previous caller's trace id without a header"
    );

    // A malformed header must fall back rather than drop the request's spans.
    get_version(Some("not-a-traceparent")).await;
    fastrace::flush();
    let spans = cap.take();
    assert!(
        !spans.is_empty(),
        "a malformed traceparent discarded the request's spans"
    );
    assert!(
        spans.iter().all(|s| s.trace_id != TraceId(0)),
        "malformed traceparent produced a zero trace id"
    );
}

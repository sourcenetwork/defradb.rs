//! The two properties that matter: a `root` seam works whether or not a caller
//! already started a trace, and a filtered callsite really is switchable.
//!
//! One test function, because `fastrace::set_reporter` is process-global —
//! parallel tests would overwrite each other's capture.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use defra_trace::{set_max_level, traced, Level};
use fastrace::prelude::{SpanContext, SpanRecord};

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

#[traced(name = "seam.entry", root)]
async fn seam() -> u32 {
    child().await
}

#[traced(name = "seam.child")]
async fn child() -> u32 {
    7
}

#[traced(name = "sync.verbose", level = "trace")]
fn verbose_sync() {}

#[test]
fn traced_roots_and_filters() {
    let cap = Captured::default();
    fastrace::set_reporter(cap.clone(), fastrace::collector::Config::default());
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");

    // 1. No caller trace: the seam must self-root. Before `span_or_root` this
    //    whole subtree was discarded.
    assert_eq!(rt.block_on(seam()), 7);
    fastrace::flush();
    let names: Vec<_> = cap.take().iter().map(|s| s.name.to_string()).collect();
    assert!(
        names.contains(&"seam.entry".to_string()) && names.contains(&"seam.child".to_string()),
        "seam did not self-root: {names:?}"
    );

    // 2. With a caller trace, the seam must join it rather than start a second
    //    one, or each request would yield two disconnected traces.
    let caller = SpanContext::random();
    rt.block_on(async {
        use fastrace::future::FutureExt as _;
        let root = fastrace::Span::root("caller", caller);
        async { seam().await }.in_span(root).await;
    });
    fastrace::flush();
    let spans = cap.take();
    let ids: HashSet<_> = spans.iter().map(|s| s.trace_id).collect();
    assert_eq!(
        ids.len(),
        1,
        "seam split the trace: {:?}",
        spans
            .iter()
            .map(|s| (&s.name, s.trace_id))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ids.into_iter().next().expect("one trace"),
        caller.trace_id,
        "seam did not inherit the caller's trace id"
    );

    // 3. A trace-level callsite is silent at the default Info ceiling.
    {
        let root = fastrace::Span::root("r1", SpanContext::random());
        let _g = root.set_local_parent();
        verbose_sync();
    }
    fastrace::flush();
    let names: Vec<_> = cap.take().iter().map(|s| s.name.to_string()).collect();
    assert!(
        !names.contains(&"sync.verbose".to_string()),
        "verbose site leaked at Info: {names:?}"
    );

    // 4. Raising the ceiling must re-resolve it, not reuse the first decision.
    set_max_level(Level::Trace);
    {
        let root = fastrace::Span::root("r2", SpanContext::random());
        let _g = root.set_local_parent();
        verbose_sync();
    }
    fastrace::flush();
    let names: Vec<_> = cap.take().iter().map(|s| s.name.to_string()).collect();
    assert!(
        names.contains(&"sync.verbose".to_string()),
        "verbose site stayed gated after raising the level: {names:?}"
    );

    // 5. And lowering it again must silence it.
    set_max_level(Level::Info);
    {
        let root = fastrace::Span::root("r3", SpanContext::random());
        let _g = root.set_local_parent();
        verbose_sync();
    }
    fastrace::flush();
    let names: Vec<_> = cap.take().iter().map(|s| s.name.to_string()).collect();
    assert!(
        !names.contains(&"sync.verbose".to_string()),
        "verbose site leaked after re-disabling: {names:?}"
    );
}

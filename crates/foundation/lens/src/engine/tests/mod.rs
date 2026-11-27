//! The engine's contract: one engine, one linker and one clock shared by
//! every clone; a call that yields on every tick and traps once its ticks are
//! spent; calls run to completion bounded by their permits; and a clock that
//! stops with the last clone.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::executor::block_on;
use futures::task::{waker, ArcWake};
use futures::{Stream, StreamExt, TryStreamExt};
use serde_json::json;
use wasmtime::Module;

use super::call::linkers_built;
use super::{CompiledModule, LensEngine, Pipeline, ThreadTicks, WasmSandboxConfig};
use crate::Error;

mod probe;

/// Imports `lens::next`; `alloc` returns one in-bounds scratch offset and
/// `transform` returns the next input record, so the module passes every
/// document through the real host path.
pub(crate) const IDENTITY_PASSTHROUGH_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
    0x01, 0x0a, 0x02, 0x60, 0x00, 0x01, 0x7f, 0x60, 0x01, 0x7f, 0x01, 0x7f, // types
    0x02, 0x0d, 0x01, 0x04, b'l', b'e', b'n', b's', 0x04, b'n', b'e', b'x', b't', 0x00,
    0x00, // import lens.next
    0x03, 0x03, 0x02, 0x01, 0x00, // alloc and transform functions
    0x05, 0x03, 0x01, 0x00, 0x01, // one-page memory
    0x07, 0x1e, 0x03, 0x06, b'm', b'e', b'm', b'o', b'r', b'y', 0x02, 0x00, 0x05, b'a', b'l', b'l',
    b'o', b'c', 0x00, 0x01, 0x09, b't', b'r', b'a', b'n', b's', b'f', b'o', b'r', b'm', 0x00,
    0x02, // exports
    0x0a, 0x0c, 0x02, 0x05, 0x00, 0x41, 0x80, 0x08, 0x0b, 0x04, 0x00, 0x10, 0x00,
    0x0b, // code: alloc returns 1024; transform calls lens.next
];

/// The identity module's shape, with a `transform` that loops forever.
pub(crate) const INFINITE_TRANSFORM_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
    0x01, 0x0a, 0x02, 0x60, 0x00, 0x01, 0x7f, 0x60, 0x01, 0x7f, 0x01, 0x7f, // types
    0x02, 0x0d, 0x01, 0x04, b'l', b'e', b'n', b's', 0x04, b'n', b'e', b'x', b't', 0x00,
    0x00, // import lens.next
    0x03, 0x03, 0x02, 0x01, 0x00, // alloc and transform functions
    0x05, 0x03, 0x01, 0x00, 0x01, // one-page memory
    0x07, 0x1e, 0x03, 0x06, b'm', b'e', b'm', b'o', b'r', b'y', 0x02, 0x00, 0x05, b'a', b'l', b'l',
    b'o', b'c', 0x00, 0x01, 0x09, b't', b'r', b'a', b'n', b's', b'f', b'o', b'r', b'm', 0x00,
    0x02, // exports
    0x0a, 0x11, 0x02, 0x05, 0x00, 0x41, 0x80, 0x08, 0x0b, // alloc returns 1024
    0x09, 0x00, 0x03, 0x40, 0x0c, 0x00, 0x0b, 0x41, 0x00, 0x0b, // loop { br 0 }; i32.const 0
];

/// Counts the wakes of the task polling a call.
#[derive(Default)]
struct Wakes(AtomicUsize);

impl ArcWake for Wakes {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn sandbox(ticks: u64, period: Duration) -> WasmSandboxConfig {
    WasmSandboxConfig {
        max_memory_bytes: Some(1 << 20),
        fuel_budget: None,
        epoch_deadline_ticks: Some(ticks),
        epoch_tick_period: Some(period),
        max_concurrent_calls: None,
    }
}

fn pipeline(engine: &LensEngine, wasm: &[u8]) -> Pipeline {
    let module = Module::new(engine.engine(), wasm).expect("test module compiles");
    let pre = engine.prepare(&module).expect("test module links");
    Pipeline::from(vec![CompiledModule::new(pre, None, false)])
}

fn one_doc() -> impl Stream<Item = serde_json::Value> + Send + 'static {
    futures::stream::iter([json!({"name": "Ada"})])
}

fn poll<F>(future: Pin<&mut F>, wakes: &Arc<Wakes>) -> Poll<F::Output>
where
    F: Future + ?Sized,
{
    let waker = waker(Arc::clone(wakes));
    future.poll(&mut Context::from_waker(&waker))
}

#[test]
fn a_call_yields_to_its_executor_on_every_tick() {
    let engine = LensEngine::new(
        Some(sandbox(u64::MAX, Duration::from_millis(1))),
        &ThreadTicks,
    )
    .unwrap();
    let mut call = engine.call(
        pipeline(&engine, INFINITE_TRANSFORM_WASM),
        one_doc(),
        false,
        None,
    );
    let wakes = Arc::new(Wakes::default());

    for yields in 1..=5 {
        let mut next = call.next();
        assert!(poll(Pin::new(&mut next), &wakes).is_pending());
        assert_eq!(
            wakes.0.load(Ordering::SeqCst),
            yields,
            "each pending poll is a yield that wakes its task at once"
        );
    }
}

#[test]
fn a_call_traps_once_its_ticks_are_spent() {
    let engine = LensEngine::new(Some(sandbox(3, Duration::from_millis(1))), &ThreadTicks).unwrap();
    let mut call = engine.call(
        pipeline(&engine, INFINITE_TRANSFORM_WASM),
        one_doc(),
        false,
        None,
    );
    let wakes = Arc::new(Wakes::default());

    let mut yields = 0;
    let outcome = loop {
        let mut next = call.next();
        match poll(Pin::new(&mut next), &wakes) {
            Poll::Pending => yields += 1,
            Poll::Ready(outcome) => break outcome,
        }
    };
    assert_eq!(yields, 2, "three ticks: two yields, then the trap");
    assert!(matches!(outcome, Some(Err(Error::TimeLimit))));
    assert!(block_on(call.next()).is_none(), "a failed call ends");
}

#[test]
fn a_document_taken_in_refills_the_fuel() {
    let mut sandbox = WasmSandboxConfig::restrictive();
    // Enough for a few documents, far too little for the whole stream at once.
    sandbox.fuel_budget = Some(10_000);
    let engine = LensEngine::new(Some(sandbox), &ThreadTicks).unwrap();
    let docs: Vec<_> = (0..2_000).map(|n| json!({ "n": n })).collect();
    let output: Vec<serde_json::Value> = block_on(
        engine
            .call(
                pipeline(&engine, IDENTITY_PASSTHROUGH_WASM),
                futures::stream::iter(docs.clone()),
                false,
                None,
            )
            .try_collect(),
    )
    .unwrap();
    assert_eq!(output, docs);
}

#[test]
fn a_call_streams_an_input_larger_than_its_channels() {
    let engine = LensEngine::new(None, &ThreadTicks).unwrap();
    let pulled = Arc::new(AtomicUsize::new(0));
    let padding = "x".repeat(10 * 1024);
    let count = 1_000;
    let input = {
        let pulled = Arc::clone(&pulled);
        let padding = padding.clone();
        futures::stream::iter(0..count).map(move |n| {
            pulled.fetch_add(1, Ordering::SeqCst);
            json!({ "n": n, "padding": padding })
        })
    };
    let mut call = engine.call(
        pipeline(&engine, IDENTITY_PASSTHROUGH_WASM),
        input,
        false,
        None,
    );

    let first = block_on(call.next()).unwrap().unwrap();
    assert_eq!(first["n"], 0);
    let in_flight = pulled.load(Ordering::SeqCst);
    // About 10 MiB of input; two channels of 1 MiB hold about 200 of them.
    assert!(
        in_flight < 300,
        "the call read {in_flight} documents ahead, it must not collect its input"
    );

    let rest: Vec<serde_json::Value> = block_on(call.try_collect()).unwrap();
    assert_eq!(rest.len(), count - 1);
    assert!(rest.iter().enumerate().all(|(n, doc)| doc["n"] == n + 1));
}

#[test]
fn calls_run_to_completion_wait_for_a_permit() {
    let mut sandbox = sandbox(u64::MAX, Duration::from_millis(1));
    sandbox.max_concurrent_calls = Some(1);
    let engine = LensEngine::new(Some(sandbox), &ThreadTicks).unwrap();
    let wakes = Arc::new(Wakes::default());

    let holder = {
        let engine = engine.clone();
        let pipeline = pipeline(&engine, INFINITE_TRANSFORM_WASM);
        async move {
            let permit = engine.permit(pipeline.len()).await;
            engine.call(pipeline, one_doc(), false, permit).next().await
        }
    };
    let mut holder = Box::pin(holder);
    assert!(
        poll(holder.as_mut(), &wakes).is_pending(),
        "it runs and yields"
    );

    let waiter = {
        let engine = engine.clone();
        let pipeline = pipeline(&engine, IDENTITY_PASSTHROUGH_WASM);
        async move {
            let permit = engine.permit(pipeline.len()).await;
            engine.call(pipeline, one_doc(), false, permit).next().await
        }
    };
    let mut waiter = Box::pin(waiter);
    assert!(
        poll(waiter.as_mut(), &wakes).is_pending(),
        "the identity call would finish at once; it waits for the permit"
    );

    drop(holder);
    match poll(waiter.as_mut(), &wakes) {
        Poll::Ready(Some(Ok(doc))) => assert_eq!(doc, json!({"name": "Ada"})),
        other => panic!("the waiter runs once the permit is free: {other:?}"),
    }
}

#[test]
fn calls_build_no_linker() {
    let before = linkers_built();
    let engine = LensEngine::new(None, &ThreadTicks).unwrap();
    assert_eq!(
        linkers_built(),
        before + 1,
        "the engine builds its one linker"
    );

    let identity = pipeline(&engine, IDENTITY_PASSTHROUGH_WASM);
    let shared = engine.clone();
    for _ in 0..10 {
        let output: Vec<serde_json::Value> = block_on(
            shared
                .call(Arc::clone(&identity), one_doc(), false, None)
                .try_collect(),
        )
        .unwrap();
        assert_eq!(output, vec![json!({"name": "Ada"})]);
    }
    assert_eq!(linkers_built(), before + 1, "no call built a linker");
}

#[test]
fn a_shared_engine_interrupts_an_endless_call_and_stops_its_clock_with_the_last_clone() {
    let base = LensEngine::new(Some(sandbox(1, Duration::from_millis(2))), &ThreadTicks).unwrap();
    let shared = base.clone();
    assert!(wasmtime::Engine::same(base.engine(), shared.engine()));
    let endless = pipeline(&shared, INFINITE_TRANSFORM_WASM);
    let clock = shared.ticks_handle().unwrap();
    drop(base);

    let (result_tx, result_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let outcome = block_on(shared.call(endless, one_doc(), false, None).next());
        drop(shared);
        let _ = result_tx.send(outcome);
    });

    let outcome = result_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("epoch interruption must finish within the bounded test window");
    assert!(matches!(outcome, Some(Err(Error::TimeLimit))));
    assert!(clock.upgrade().is_none(), "the last clone stops the clock");
}

#[test]
fn an_engine_without_a_sandbox_runs_no_clock() {
    let engine = LensEngine::new(None, &ThreadTicks).unwrap();
    assert!(engine.ticks_handle().is_none());
    assert!(engine.sandbox().is_none());
}

#[test]
fn an_epoch_deadline_needs_a_positive_tick_period() {
    let mut sandbox = WasmSandboxConfig::restrictive();
    sandbox.epoch_tick_period = Some(Duration::ZERO);
    assert!(matches!(
        LensEngine::new(Some(sandbox.clone()), &ThreadTicks),
        Err(Error::InvalidConfig(_))
    ));
    sandbox.epoch_tick_period = None;
    assert!(matches!(
        LensEngine::new(Some(sandbox), &ThreadTicks),
        Err(Error::InvalidConfig(_))
    ));
}

#[test]
fn a_call_bound_needs_at_least_one_call() {
    let mut sandbox = WasmSandboxConfig::restrictive();
    sandbox.max_concurrent_calls = Some(0);
    assert!(matches!(
        LensEngine::new(Some(sandbox), &ThreadTicks),
        Err(Error::InvalidConfig(_))
    ));
}

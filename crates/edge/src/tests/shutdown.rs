use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use super::{poll_once, runtime, runtime_with, set_soon, single_threaded, stuck, DropFlag};
use crate::{EdgeConfig, EdgeError};

#[test]
fn shutdown_stops_accepting_drains_then_stops_the_rest() {
    let runtime = runtime_with(EdgeConfig {
        drain: Duration::from_millis(100),
        ..EdgeConfig::default()
    });
    let edge = runtime.edge().clone();

    let finished = Arc::new(AtomicBool::new(false));
    let watcher = edge.clone();
    let finishing = Arc::clone(&finished);
    drop(edge.spawn(async move {
        watcher.closing().await;
        finishing.store(true, Ordering::SeqCst);
    }));
    let (flag, stopped) = DropFlag::new();
    let (_hold, stuck) = stuck();
    drop(edge.spawn(async move {
        let _flag = flag;
        stuck.await
    }));

    let drained = runtime.shutdown().unwrap();

    // Stop accepting woke the closing watcher, which finished within the
    // drain; the stuck task outlived it, and is dropped once woken.
    assert_eq!(drained.remaining, 1);
    assert!(finished.load(Ordering::SeqCst));
    assert!(!stopped.load(Ordering::SeqCst));
    drop(_hold);
    assert!(set_soon(&stopped), "the woken task is dropped");
    assert!(edge.is_closed());
    assert_eq!(
        poll_once(edge.spawn(async { 1 })),
        Poll::Ready(Err(EdgeError::Closed))
    );
    assert_eq!(edge.block_on(async { 1 }), Err(EdgeError::Closed));
    assert_eq!(edge.stats().refused, 2);
}

#[test]
fn shutdown_inside_another_runtime_waits_until_the_runtime_is_gone() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let other = single_threaded();

    let outcome = other.block_on(async move { runtime.shutdown() });

    assert!(outcome.unwrap().is_complete());
    assert!(edge.is_closed());
}

#[test]
fn dropping_the_runtime_inside_an_async_context_is_safe() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let current = single_threaded();

    current.block_on(async move { drop(runtime) });

    assert!(edge.is_closed());
}

#[test]
fn an_awaited_shutdown_completes_once_the_runtime_is_gone() {
    let runtime = runtime();
    let current = single_threaded();

    let drained = current.block_on(runtime.shutdown_async()).unwrap();

    assert!(drained.is_complete());
}

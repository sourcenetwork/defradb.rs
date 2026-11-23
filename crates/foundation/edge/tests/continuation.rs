use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use edge::{Edge, EdgeConfig, EdgeError, EdgeRuntime};
use regolith::sync::Event;

const LIMIT: Duration = Duration::from_secs(10);

fn start() -> EdgeRuntime {
    EdgeRuntime::start(EdgeConfig {
        threads: NonZeroUsize::new(2),
        thread_name: "edge-continuation".to_string(),
        drain: LIMIT,
        stop: Duration::from_secs(1),
        ..EdgeConfig::default()
    })
    .unwrap()
}

fn hold(edge: &Edge, release: &Arc<Event>) -> edge::Task<()> {
    let release = Arc::clone(release);
    edge.spawn(async move { release.wait().await })
}

#[test]
fn a_continuation_is_admitted_after_close_while_admitted_work_runs() {
    let runtime = start();
    let edge = runtime.edge().clone();
    let scope = edge.scope();
    let release = Arc::new(Event::new());
    let request = hold(scope.edge(), &release);
    scope.close();

    let refused = scope.edge().spawn(async {});
    let continued = scope.edge().spawn_continuation({
        let release = Arc::clone(&release);
        async move {
            release.wait().await;
            7
        }
    });
    edge.block_on(async {
        assert_eq!(refused.await, Err(EdgeError::Closed));
        release.set();
        assert_eq!(continued.await, Ok(7));
        request.await.unwrap();
        assert!(scope.drain(LIMIT).await.is_complete());
    })
    .unwrap();
    runtime.shutdown().unwrap();
}

#[test]
fn a_continuation_is_refused_after_close_when_nothing_is_in_flight() {
    let runtime = start();
    let edge = runtime.edge().clone();
    let scope = edge.scope();
    scope.close();
    let refused = scope.edge().spawn_continuation(async {});
    edge.block_on(async {
        assert_eq!(refused.await, Err(EdgeError::Closed));
    })
    .unwrap();
    runtime.shutdown().unwrap();
}

#[test]
fn a_continuation_is_admitted_before_close_like_a_plain_spawn() {
    let runtime = start();
    let edge = runtime.edge().clone();
    let scope = edge.scope();
    let continued = scope.edge().spawn_continuation(async { 3 });
    edge.block_on(async {
        assert_eq!(continued.await, Ok(3));
        assert!(scope.drain(LIMIT).await.is_complete());
    })
    .unwrap();
    runtime.shutdown().unwrap();
}

#[test]
fn the_drain_waits_for_a_running_continuation() {
    let runtime = start();
    let edge = runtime.edge().clone();
    let scope = edge.scope();
    let release = Arc::new(Event::new());
    let request = hold(scope.edge(), &release);
    scope.close();
    let continued = scope.edge().spawn_continuation({
        let release = Arc::clone(&release);
        async move { release.wait().await }
    });
    edge.block_on(async {
        let early = scope.drain(Duration::from_millis(100)).await;
        assert_eq!(early.remaining, 2, "{early:?}");
        release.set();
        request.await.unwrap();
        continued.await.unwrap();
        assert!(scope.drain(LIMIT).await.is_complete());
    })
    .unwrap();
    runtime.shutdown().unwrap();
}

#[test]
fn a_continuation_is_refused_after_the_drain_completed_even_with_plain_spawns_racing() {
    let runtime = start();
    let edge = runtime.edge().clone();
    let scope = edge.scope();
    let release = Arc::new(Event::new());
    let request = hold(scope.edge(), &release);
    scope.close();
    let stop = Arc::new(AtomicBool::new(false));
    let racer = {
        let edge = scope.edge().clone();
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                drop(edge.spawn(async {}));
            }
        })
    };
    release.set();
    edge.block_on(async {
        request.await.unwrap();
        assert!(scope.drain(LIMIT).await.is_complete());
    })
    .unwrap();
    for _ in 0..1000 {
        let late = scope.edge().spawn_continuation(async {});
        edge.block_on(async { assert_eq!(late.await, Err(EdgeError::Closed)) })
            .unwrap();
    }
    stop.store(true, Ordering::SeqCst);
    racer.join().unwrap();
    runtime.shutdown().unwrap();
}

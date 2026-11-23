use std::future::pending;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use super::{poll_once, runtime, runtime_with, set_soon, stuck, DropFlag};
use crate::{EdgeConfig, EdgeError};

#[test]
fn a_scope_stops_and_drains_on_its_own() {
    let runtime = runtime_with(EdgeConfig {
        drain: Duration::from_millis(20),
        ..EdgeConfig::default()
    });
    let edge = runtime.edge().clone();
    let scope = edge.scope();
    let (flag, stopped) = DropFlag::new();
    let (_hold, stuck) = stuck();
    drop(scope.edge().spawn(async move {
        let _flag = flag;
        stuck.await
    }));

    let drained = edge
        .block_on(scope.drain(Duration::from_millis(20)))
        .unwrap();

    assert_eq!(drained.remaining, 1);
    assert!(scope.edge().is_closed());
    assert!(!edge.is_closed());
    assert_eq!(
        poll_once(scope.edge().spawn(async { 1 })),
        Poll::Ready(Err(EdgeError::Closed))
    );
    assert_eq!(edge.block_on(edge.spawn(async { 2 })).unwrap(), Ok(2));
    assert_eq!(poll_once(scope.edge().closing()), Poll::Ready(()));
    assert_eq!(poll_once(edge.closing()), Poll::Pending);

    // The runtime's own drain still counts the scope's stuck work, which
    // the shutdown then drops without anything waking it.
    assert_eq!(runtime.shutdown().unwrap().remaining, 1);
    assert!(
        set_soon(&stopped),
        "the stuck task is dropped by the shutdown"
    );
}

#[test]
fn closing_the_runtime_closes_every_scope() {
    let runtime = runtime();
    let scope = runtime.edge().scope();
    let scoped = scope.edge().clone();

    assert!(runtime.shutdown().unwrap().is_complete());

    assert!(scoped.is_closed());
    assert_eq!(poll_once(scoped.closing()), Poll::Ready(()));
}

#[test]
fn a_service_stops_when_its_handle_drops_or_its_scope_closes() {
    let runtime = runtime();
    let edge = runtime.edge().clone();

    let dropped_scope = edge.scope();
    let (flag, dropped) = DropFlag::new();
    let service = dropped_scope.edge().spawn_service(async move {
        let _flag = flag;
        pending::<()>().await
    });
    assert!(service.is_running());
    drop(service);
    let drained = edge
        .block_on(dropped_scope.drain(Duration::from_secs(5)))
        .unwrap();
    assert!(drained.is_complete());
    assert!(dropped.load(Ordering::SeqCst));

    let closed_scope = edge.scope();
    let (flag, closed) = DropFlag::new();
    closed_scope
        .edge()
        .spawn_service(async move {
            let _flag = flag;
            pending::<()>().await
        })
        .detach();
    let drained = edge
        .block_on(closed_scope.drain(Duration::from_secs(5)))
        .unwrap();
    assert!(drained.is_complete());
    assert!(closed.load(Ordering::SeqCst));
}

#[test]
fn a_stopped_service_has_let_go_of_what_it_held() {
    let runtime = runtime();
    let edge = runtime.edge().clone();

    let (flag, released) = DropFlag::new();
    let service = edge.spawn_service(async move {
        let _flag = flag;
        pending::<()>().await
    });
    assert!(service.is_running());
    edge.block_on(service.stopped()).unwrap();
    assert!(released.load(Ordering::SeqCst));
}

/// A service that owns a server's connection set keeps it through the scope's
/// drain, so work already admitted finishes instead of being dropped at once.
#[test]
fn a_service_drains_its_task_set_when_its_scope_closes() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let scope = edge.scope();
    let service_edge = scope.edge().clone();
    let task_edge = service_edge.clone();
    let closing_edge = service_edge.clone();
    let finished = Arc::new(AtomicBool::new(false));
    let task_finished = Arc::clone(&finished);
    let (started, ready) = futures::channel::oneshot::channel();

    let service = service_edge.spawn(async move {
        let mut connections = crate::TaskSet::new(task_edge.clone());
        connections.spawn(async move {
            let _ = started.send(());
            task_edge.sleep(Duration::from_millis(50)).await;
            task_finished.store(true, Ordering::SeqCst);
        });

        let mut closing = pin!(closing_edge.closing());
        loop {
            nuclei::select! {
                Some(_) = connections.join_next() => continue,
                () = closing.as_mut() => break,
            }
        }
        while connections.join_next().await.is_some() {}
    });
    edge.block_on(ready).unwrap().unwrap();
    let drained = edge.block_on(scope.drain(Duration::from_secs(5))).unwrap();
    assert!(drained.is_complete());
    assert!(finished.load(Ordering::SeqCst));
    assert!(edge.block_on(service).is_ok());
}

#[test]
fn a_closed_scope_hands_the_work_back_unstarted() {
    let runtime = runtime();
    let scope = runtime.edge().scope();
    scope.close();
    let (flag, dropped) = DropFlag::new();
    let work = async move {
        let _flag = flag;
        7
    };

    let Err(back) = scope.edge().try_spawn(work) else {
        panic!("a closed scope admits nothing");
    };

    assert!(!dropped.load(Ordering::SeqCst), "handed back, not dropped");
    assert_eq!(runtime.edge().block_on(back).unwrap(), 7);
    assert!(dropped.load(Ordering::SeqCst));
    assert!(runtime.shutdown().unwrap().is_complete());
}

use std::future::pending;
use std::sync::atomic::Ordering;
use std::task::Poll;
use std::time::Duration;

use super::{poll_once, runtime, runtime_with, stuck, DropFlag};
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
    // is dropped once woken.
    assert_eq!(runtime.shutdown().unwrap().remaining, 1);
    drop(_hold);
    assert!(stopped.load(Ordering::SeqCst));
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

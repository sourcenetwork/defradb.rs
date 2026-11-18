use std::future::pending;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::{explode, runtime, runtime_with, DropFlag};
use crate::{EdgeConfig, EdgeError, TaskSet};

#[test]
fn a_set_hands_back_each_task_as_it_ends() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let mut set = TaskSet::new(edge.clone());

    set.spawn(async { 1 });
    set.spawn(async { 2 });
    assert_eq!(set.len(), 2);

    let mut ended = edge
        .block_on(async {
            let mut ended = Vec::new();
            while let Some(outcome) = set.join_next().await {
                ended.push(outcome);
            }
            ended
        })
        .unwrap();
    ended.sort_by_key(|outcome| *outcome.as_ref().unwrap_or(&0));

    assert_eq!(ended, vec![Ok(1), Ok(2)]);
    assert!(set.is_empty());
}

#[test]
fn a_task_of_a_set_outlives_the_runtimes_deadline() {
    let runtime = runtime_with(EdgeConfig {
        deadline: Duration::from_millis(20),
        ..EdgeConfig::default()
    });
    let edge = runtime.edge().clone();
    let mut set = TaskSet::new(edge.clone());
    let timer = edge.clone();

    set.spawn(async move {
        timer.sleep(Duration::from_millis(80)).await;
        7
    });

    assert_eq!(edge.block_on(set.join_next()).unwrap(), Some(Ok(7)));
}

#[test]
fn a_panic_ends_only_its_own_task() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let mut set = TaskSet::new(edge.clone());

    set.spawn(explode());

    assert!(matches!(
        edge.block_on(set.join_next()).unwrap(),
        Some(Err(EdgeError::Panicked(_)))
    ));
}

#[test]
fn dropping_a_set_stops_its_tasks() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let mut set = TaskSet::new(edge);
    let (flag, stopped) = DropFlag::new();

    set.spawn(async move {
        let _flag = flag;
        pending::<()>().await
    });
    drop(set);

    // The drain completes only once the stopped task released its admission.
    assert!(runtime.shutdown().unwrap().is_complete());
    assert!(stopped.load(Ordering::SeqCst));
}

#[test]
fn aborting_a_set_stops_its_tasks_and_hands_each_back_cancelled() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let mut set = TaskSet::new(edge.clone());
    let (flag, stopped) = DropFlag::new();

    set.spawn(async move {
        let _flag = flag;
        pending::<()>().await
    });
    set.abort_all();

    assert_eq!(
        edge.block_on(set.join_next()).unwrap(),
        Some(Err(EdgeError::Cancelled))
    );
    assert!(stopped.load(Ordering::SeqCst));
    assert!(set.is_empty());
}

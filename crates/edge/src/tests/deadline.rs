use std::future::pending;
use std::sync::atomic::Ordering;
use std::task::Poll;
use std::time::Duration;

use super::{poll_once, runtime, runtime_with, stuck, DropFlag};
use crate::{EdgeConfig, EdgeError};

#[test]
fn a_completion_past_its_deadline_reports_it_and_stops_the_work() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let (flag, stopped) = DropFlag::new();
    let limit = Duration::from_millis(20);

    let task = edge
        .spawn(async move {
            let _flag = flag;
            pending::<()>().await
        })
        .within(limit);

    assert_eq!(
        edge.block_on(task).unwrap(),
        Err(EdgeError::Deadline(limit))
    );
    assert_eq!(edge.stats().deadlines, 1);
    // The drain completes only once the stopped task released its admission.
    assert!(runtime.shutdown().unwrap().is_complete());
    assert!(stopped.load(Ordering::SeqCst));
}

#[test]
fn every_completion_carries_the_configured_deadline_by_default() {
    let limit = Duration::from_millis(20);
    let runtime = runtime_with(EdgeConfig {
        deadline: limit,
        ..EdgeConfig::default()
    });
    let edge = runtime.edge().clone();

    let task = edge.spawn(pending::<()>());

    assert_eq!(edge.deadline(), limit);
    assert_eq!(
        edge.block_on(task).unwrap(),
        Err(EdgeError::Deadline(limit))
    );
}

#[test]
fn a_timeout_gives_the_value_in_time_and_the_deadline_otherwise() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let limit = Duration::from_millis(20);

    let in_time = edge.block_on(edge.timeout(limit, async { 3 })).unwrap();
    let too_late = edge
        .block_on(edge.timeout(limit, pending::<u32>()))
        .unwrap();

    assert_eq!(in_time, Ok(3));
    assert_eq!(too_late, Err(EdgeError::Deadline(limit)));
}

#[test]
fn a_completion_with_no_deadline_waits_for_its_work_until_the_runtime_stops() {
    let runtime = runtime_with(EdgeConfig {
        deadline: Duration::from_millis(1),
        drain: Duration::from_millis(20),
        ..EdgeConfig::default()
    });
    let edge = runtime.edge().clone();
    let sleeper = edge.clone();

    let slow = edge
        .spawn(async move {
            sleeper.sleep(Duration::from_millis(30)).await;
            5
        })
        .within(Duration::MAX);
    assert_eq!(edge.block_on(slow).unwrap(), Ok(5));

    let (_hold, stuck) = stuck();
    let task = edge.spawn(stuck).within(Duration::MAX);
    assert_eq!(runtime.shutdown().unwrap().remaining, 1);
    assert_eq!(poll_once(task), Poll::Ready(Err(EdgeError::Closed)));
}

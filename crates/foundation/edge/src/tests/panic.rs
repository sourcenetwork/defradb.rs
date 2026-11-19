use super::{explode, runtime};
use crate::EdgeError;

#[test]
fn a_panicking_task_fails_only_its_own_completion() {
    let runtime = runtime();
    let edge = runtime.edge().clone();

    let panicking = edge.spawn(explode());
    let healthy = edge.spawn(async { 7 });

    assert_eq!(
        edge.block_on(panicking).unwrap(),
        Err(EdgeError::Panicked("boom".to_string()))
    );
    assert_eq!(edge.block_on(healthy).unwrap(), Ok(7));
    assert_eq!(edge.block_on(edge.spawn(async { 8 })).unwrap(), Ok(8));
    assert_eq!(edge.stats().panicked, 1);
}

#[test]
fn panics_in_blocking_and_pinned_work_are_contained_too() {
    let runtime = runtime();
    let edge = runtime.edge().clone();

    let blocking = edge.spawn_blocking(|| -> u32 { panic!("blocking boom") });
    let pinned = edge.spawn_pinned(explode);

    assert_eq!(
        edge.block_on(blocking).unwrap(),
        Err(EdgeError::Panicked("blocking boom".to_string()))
    );
    assert_eq!(
        edge.block_on(pinned).unwrap(),
        Err(EdgeError::Panicked("boom".to_string()))
    );
    assert_eq!(edge.stats().panicked, 2);
}

#[test]
fn a_detached_panicking_task_is_counted_where_it_panicked() {
    let runtime = runtime();
    let edge = runtime.edge().clone();

    drop(edge.spawn(explode()));

    assert!(runtime.shutdown().unwrap().is_complete());
    assert_eq!(edge.stats().panicked, 1);
}

#[test]
fn a_panic_under_block_on_is_returned_to_the_caller() {
    let runtime = runtime();
    let edge = runtime.edge().clone();

    assert_eq!(
        edge.block_on(explode()),
        Err(EdgeError::Panicked("boom".to_string()))
    );
}

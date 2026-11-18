use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use super::runtime;
use crate::Drained;

#[test]
fn a_spawned_task_hands_back_its_value() {
    let runtime = runtime();
    let edge = runtime.edge().clone();

    let value = edge.block_on(edge.spawn(async { 6 * 7 })).unwrap();

    assert_eq!(value, Ok(42));
    assert_eq!(runtime.shutdown(), Ok(Drained { remaining: 0 }));
}

#[test]
fn blocking_work_hands_back_its_value() {
    let runtime = runtime();
    let edge = runtime.edge().clone();

    let value = edge.block_on(edge.spawn_blocking(|| 6 * 7)).unwrap();

    assert_eq!(value, Ok(42));
}

thread_local! {
    static MARK: Cell<u32> = const { Cell::new(0) };
}

#[test]
fn a_pinned_future_stays_on_the_thread_its_maker_ran_on() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let timer = edge.clone();

    let task = edge.spawn_pinned(move || {
        MARK.with(|mark| mark.set(9));
        let not_send = Rc::new(5);
        async move {
            timer.sleep(Duration::from_millis(1)).await;
            MARK.with(Cell::get) + *not_send
        }
    });

    assert_eq!(edge.block_on(task).unwrap(), Ok(14));
}

use super::runtime;

#[test]
fn block_on_inside_another_runtime_nests_and_gives_the_value() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let other = super::single_threaded();

    let nested = other.block_on(async { edge.block_on(async { 1 }) });

    assert_eq!(nested, Ok(1));
}

#[test]
fn block_on_from_a_task_of_the_runtime_hands_its_thread_over() {
    let runtime = runtime();
    let edge = runtime.edge().clone();
    let inner = edge.clone();

    let nested = edge.spawn(async move { inner.block_on(async { 3 }) });

    assert_eq!(edge.block_on(nested).unwrap(), Ok(Ok(3)));
}

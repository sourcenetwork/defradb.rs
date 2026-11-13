use std::time::Instant;

use super::*;

const METHOD: &str = "vera_getReceiptProof";

#[nuclei::test]
async fn an_answer_that_never_comes_ends_the_wait_at_the_deadline() {
    let edge = edge::testing::edge();
    let deadline = Duration::from_millis(40);
    let started = Instant::now();
    let outcome = bounded(&edge, METHOD, deadline, std::future::pending::<()>()).await;
    assert!(matches!(
        outcome,
        Err(Failure::Elapsed { ref method, deadline: waited })
            if method == METHOD && waited == deadline
    ));
    assert!(started.elapsed() >= deadline);
}

#[nuclei::test]
async fn an_answer_in_time_is_returned_as_it_is() {
    let edge = edge::testing::edge();
    for deadline in [Duration::from_secs(5), Duration::ZERO] {
        let answer = bounded(&edge, METHOD, deadline, std::future::ready(7)).await;
        assert!(matches!(answer, Ok(7)));
    }
}

/// The light client polls its transport from threads of its own, not from
/// the network runtime's.
#[test]
fn the_deadline_holds_for_a_wait_polled_off_the_network_runtime() {
    let edge = edge::testing::edge();
    let deadline = Duration::from_millis(40);
    let outcome = std::thread::spawn(move || {
        futures::executor::block_on(bounded(
            &edge,
            METHOD,
            deadline,
            std::future::pending::<()>(),
        ))
    })
    .join()
    .expect("waiting thread");
    assert!(matches!(
        outcome,
        Err(Failure::Elapsed { deadline: waited, .. }) if waited == deadline
    ));
}

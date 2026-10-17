use std::future::{poll_fn, Future};
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::Poll;

use query::QueryRequest;

mod executors;

use executors::{
    context_observing_executor, slow_signing_executor, spawning_signing_executor,
    test_signing_config,
};

#[test]
fn a_signed_query_runs_on_after_its_caller_stops_waiting() {
    let started = Arc::new(AtomicBool::new(false));
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let executor = slow_signing_executor(started.clone(), completed_tx);
    let edge = edge::testing::edge();

    workers::block_on(async {
        let mut query = pin!(super::execute_with_signing_context(
            &edge,
            executor,
            QueryRequest::new("{ delayed }"),
            None,
            test_signing_config(),
            "did:key:zCancellationTest".to_string(),
        ));
        // The caller waits until the query started, then stops waiting.
        poll_fn(|cx| {
            if started.load(Ordering::SeqCst) || query.as_mut().poll(cx).is_ready() {
                return Poll::Ready(());
            }
            cx.waker().wake_by_ref();
            Poll::Pending
        })
        .await;
    })
    .unwrap();

    completed_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("a signed query must complete after its caller stopped waiting");
}

#[test]
fn work_a_signed_query_spawns_outlives_the_query() {
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let executor = spawning_signing_executor(completed_tx);
    let edge = edge::testing::edge();

    let response = workers::block_on(super::execute_with_signing_context(
        &edge,
        executor,
        QueryRequest::new("{ spawnBackgroundWork }"),
        None,
        test_signing_config(),
        "did:key:zSpawnTest".to_string(),
    ))
    .unwrap();
    assert!(
        !response.has_errors(),
        "query failed: {:?}",
        response.errors
    );

    completed_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("work spawned by a signed query must outlive query completion");
}

#[test]
fn signed_query_context_survives_awaits() {
    workers::block_on(async {
        let expected_did = "did:key:zContextTest".to_string();
        let signing_config = test_signing_config();
        let executor =
            context_observing_executor(expected_did.clone(), signing_config.public_key_hex.clone());

        let response = super::execute_with_signing_context(
            &edge::testing::edge(),
            executor,
            QueryRequest::new("{ observeContext }"),
            None,
            signing_config,
            expected_did,
        )
        .await;

        assert!(
            !response.has_errors(),
            "signed query context failed: {:?}",
            response.errors
        );
    })
    .unwrap()
}

#[test]
fn a_signed_query_after_the_node_began_shutting_down_is_refused() {
    workers::block_on(async {
        let scope = edge::testing::edge().scope();
        scope.close();
        let signing_config = test_signing_config();
        let executor = context_observing_executor(
            "did:key:zRefusedTest".to_string(),
            signing_config.public_key_hex.clone(),
        );

        let response = super::execute_with_signing_context(
            scope.edge(),
            executor,
            QueryRequest::new("{ observeContext }"),
            None,
            signing_config,
            "did:key:zRefusedTest".to_string(),
        )
        .await;

        assert_eq!(
            response.errors.first().map(|error| error.message.as_str()),
            Some("the node is shutting down")
        );
    })
    .unwrap()
}

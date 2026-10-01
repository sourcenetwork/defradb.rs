use super::ReplicatorInstalls;
use std::{
    future::{poll_fn, Future},
    task::Poll,
};

#[tokio::test]
async fn a_waiting_install_does_not_block_another_peer() {
    let installs = ReplicatorInstalls::default();
    let first = installs.acquire("first").await;
    let mut waiting = Box::pin(installs.acquire("first"));
    poll_fn(|cx| {
        assert!(waiting.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let other = installs.acquire("other").await;
    drop(first);
    let second = waiting.await;
    drop(second);
    drop(other);
    let _next = installs.acquire("next").await;
    assert_eq!(installs.peers.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn cancelled_waiter_leaves_the_install_lock_usable() {
    let installs = ReplicatorInstalls::default();
    let first = installs.acquire("peer").await;
    let mut waiting = Box::pin(installs.acquire("peer"));
    poll_fn(|cx| {
        assert!(waiting.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(waiting);
    drop(first);
    let _next = installs.acquire("peer").await;
}

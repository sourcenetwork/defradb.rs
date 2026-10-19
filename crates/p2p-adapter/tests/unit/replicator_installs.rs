use super::ReplicatorInstalls;
use std::time::Duration;

#[tokio::test]
async fn notifications_are_scoped_to_one_peer_and_install() {
    let installs = ReplicatorInstalls::default();
    let first = installs.watch("first", 1);
    let next = installs.watch("first", 2);
    let other = installs.watch("other", 1);
    installs.finish("first", 1);
    first.wait().await;
    assert!(
        n0_future::time::timeout(Duration::from_millis(1), next.wait())
            .await
            .is_err()
    );
    assert!(
        n0_future::time::timeout(Duration::from_millis(1), other.wait())
            .await
            .is_err()
    );
    drop((first, next, other));
    assert!(installs.waiters.is_empty());
}

#[tokio::test]
async fn cancelled_waiter_wakes_peers_to_recheck_without_retaining_entries() {
    let installs = ReplicatorInstalls::default();
    let first = installs.watch("peer", 1);
    let other = installs.watch("peer", 1);
    drop(first);
    other.wait().await;
    let replacement = installs.watch("peer", 1);
    drop(other);
    assert_eq!(
        installs.waiters.len(),
        1,
        "stale waiter cannot remove replacement"
    );
    installs.finish("peer", 1);
    replacement.wait().await;
    assert!(installs.waiters.is_empty());
}

//! Waiting for what the network brings, and checking what it must not.

use std::time::{Duration, Instant};

/// How long anything a test waits for may take on a loaded machine.
pub const PATIENCE: Duration = Duration::from_secs(90);

pub async fn pause() {
    edge::testing::edge()
        .sleep(Duration::from_millis(200))
        .await;
}

/// Wait until `check` holds, failing with `what` after [`PATIENCE`].
pub async fn eventually<F, Fut>(what: &str, check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    eventually_within(what, PATIENCE, check).await;
}

/// Wait until `check` holds, failing with `what` after `patience`.
pub async fn eventually_within<F, Fut>(what: &str, patience: Duration, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + patience;
    while !check().await {
        assert!(Instant::now() < deadline, "{what}");
        pause().await;
    }
}

/// Check that `check` holds throughout `period`, failing with `what` if it
/// stops holding.
pub async fn throughout<F, Fut>(what: &str, period: Duration, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let end = Instant::now() + period;
    while Instant::now() < end {
        assert!(check().await, "{what}");
        pause().await;
    }
}

/// Check that `read` gives `expected` throughout `period`, failing with
/// `what` and the value it gave instead.
pub async fn steady<T, F, Fut>(what: &str, period: Duration, expected: T, mut read: F)
where
    T: PartialEq + std::fmt::Debug,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let end = Instant::now() + period;
    while Instant::now() < end {
        let value = read().await;
        assert_eq!(value, expected, "{what}");
        pause().await;
    }
}

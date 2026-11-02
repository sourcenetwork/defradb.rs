//! The wait the integration tests share, on the shared test network runtime.
//! Included by each test crate that waits with
//! `#[path = "support/clock.rs"] mod clock;`.

use std::time::Duration;

pub async fn sleep(duration: Duration) {
    edge::testing::edge().sleep(duration).await;
}

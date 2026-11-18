#![cfg(not(target_arch = "wasm32"))]

use std::future::{poll_fn, Future};
use std::pin::pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use regolith::sync::Event;
use workers::{timeout_on, Config, Pool};

// Changing the process default must be isolated from other pool tests.
#[test]
fn configured_pool_keeps_off_worker_deadline_when_default_shuts_down() {
    let unrelated_default = Pool::start(Config::unplaced(1)).unwrap();
    assert!(unrelated_default.make_default());
    let owner = Pool::start(Config::unplaced(1)).unwrap();
    let release = Arc::new(Event::new());
    let wait = Arc::clone(&release);
    let result = workers::block_on(async {
        let mut timed = pin!(timeout_on(
            Some(&owner),
            Duration::from_secs(30),
            async move {
                wait.wait().await;
                42
            }
        ));

        poll_fn(|cx| match timed.as_mut().poll(cx) {
            Poll::Pending => Poll::Ready(()),
            Poll::Ready(_) => panic!("the gated future completed before release"),
        })
        .await;

        unrelated_default.shutdown_blocking().unwrap();
        poll_fn(|cx| match timed.as_mut().poll(cx) {
            Poll::Pending => Poll::Ready(()),
            Poll::Ready(_) => panic!("the owner timer ended with the unrelated pool"),
        })
        .await;
        release.set();
        timed.await.unwrap()
    })
    .unwrap();

    assert_eq!(result, 42);
    owner.shutdown_blocking().unwrap();
}

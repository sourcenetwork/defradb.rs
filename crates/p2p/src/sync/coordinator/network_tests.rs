//! Work a pool worker hands the coordinator runs on the network edge: a
//! worker has no reactor, so spawning there would panic and the work would
//! never run.

use std::sync::Arc;
use std::time::Duration;

use regolith::sync::Event;

use super::SyncShutdownHandle;
use crate::network::Network;

#[test]
fn a_task_spawned_from_a_pool_worker_runs_on_the_edge() {
    let edge = edge::testing::edge();
    let shutdown = SyncShutdownHandle::new(1, Network::on(edge));
    let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();

    let ran = Arc::new(Event::new());
    let done = Arc::clone(&ran);
    let handle = shutdown.clone();
    let spawned = pool
        .submit_and_wait(workers::Target::Worker(0), move || async move {
            handle.spawn_task(async move {
                assert!(!workers::on_worker(), "network work stays off the workers");
                done.set();
            })
        })
        .unwrap();
    assert!(spawned);
    workers::block_on_timeout(ran.wait(), Duration::from_secs(60)).unwrap();
    pool.shutdown_blocking().unwrap();
}

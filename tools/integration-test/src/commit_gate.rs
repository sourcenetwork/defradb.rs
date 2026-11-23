//! Holds an explicit transaction commit before it begins, so a scenario can
//! prove an HTTP wait dropped by its bound does not abandon the commit.

use std::sync::atomic::{AtomicBool, Ordering};

use defra_node::{CommitGate, CommitWait};
use regolith::sync::Event;

#[derive(Default)]
pub struct TestCommitGate {
    pub entered: Event,
    released: Event,
    completed: AtomicBool,
}

impl TestCommitGate {
    pub fn release(&self) {
        self.released.set();
    }

    pub fn completed(&self) -> bool {
        self.completed.load(Ordering::Acquire)
    }
}

impl CommitGate for TestCommitGate {
    fn enter<'gate>(&'gate self) -> CommitWait<'gate> {
        Box::pin(async move {
            self.entered.set();
            self.released.wait().await;
            self.completed.store(true, Ordering::Release);
        })
    }
}

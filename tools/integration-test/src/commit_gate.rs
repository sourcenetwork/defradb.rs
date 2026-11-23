//! Holds an explicit transaction commit before it reaches its owner, so a
//! scenario can prove a commit whose HTTP answer passed its bound still lands.

use defra_node::{CommitGate, CommitWait};
use regolith::sync::Event;

#[derive(Default)]
pub struct TestCommitGate {
    pub entered: Event,
    released: Event,
    pub ended: Event,
}

impl TestCommitGate {
    pub fn release(&self) {
        self.released.set();
    }
}

impl CommitGate for TestCommitGate {
    fn enter<'gate>(&'gate self) -> CommitWait<'gate> {
        Box::pin(async move {
            self.entered.set();
            self.released.wait().await;
        })
    }

    fn ended(&self) {
        self.ended.set();
    }
}

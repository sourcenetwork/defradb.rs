//! The index build gate a scenario starts its node with, Go's
//! `db.IndexBuildGate` hooks in `tests/integration/index/async_backfill*`:
//! it holds a backfill at its batch boundaries until the scenario opens it.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use defra_node::{GateWait, IndexBuildGate};
use regolith::sync::{Event, HopscotchMap};

use crate::actions::shared::IndexGate;

/// How long a scenario waits for a build to reach the gate.
const ENTER_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the gate holds a build before it lets it through and reports it.
const HOLD_LIMIT: Duration = Duration::from_secs(10);

pub struct TestIndexGate {
    kind: IndexGate,
    open: Event,
    /// Batches that reached the gate, the first included.
    entries: AtomicUsize,
    /// Batches the gate held.
    held: AtomicUsize,
    /// The collections whose builds reached the gate.
    collections: HopscotchMap<String, (), rapidhash::fast::RandomState>,
    /// A build waited past the hold limit and was let through.
    overheld: AtomicBool,
}

impl TestIndexGate {
    pub fn new(kind: IndexGate) -> Self {
        Self {
            kind,
            open: Event::new(),
            entries: AtomicUsize::new(0),
            held: AtomicUsize::new(0),
            collections: HopscotchMap::with_hasher(rapidhash::fast::RandomState::new()),
            overheld: AtomicBool::new(false),
        }
    }

    /// Let every held build, and every later batch, through; an error if a
    /// build was already let through for waiting past the hold limit.
    pub fn release(&self) -> Result<(), String> {
        self.open.set();
        match self.overheld.load(Ordering::Acquire) {
            true => Err(format!(
                "a build waited at the gate past {HOLD_LIMIT:?} and was let through: \
                 the node did not run while the gate held it"
            )),
            false => Ok(()),
        }
    }

    /// Wait until the gate holds a build.
    pub fn wait_blocked(&self) -> Result<(), String> {
        self.wait_for(
            || self.held.load(Ordering::Acquire) > 0,
            "a build to block at the gate",
        )
    }

    /// Wait until builds of `count` distinct collections reached the gate.
    pub fn wait_collections(&self, count: usize) -> Result<(), String> {
        self.wait_for(
            || self.collections.len() >= count,
            &format!("{count} collections' builds to reach the gate"),
        )
    }

    fn wait_for<F>(&self, reached: F, what: &str) -> Result<(), String>
    where
        F: Fn() -> bool,
    {
        let deadline = Instant::now() + ENTER_TIMEOUT;
        while !reached() {
            if Instant::now() > deadline {
                return Err(format!(
                    "timed out after {ENTER_TIMEOUT:?} waiting for {what}"
                ));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }
}

impl IndexBuildGate for TestIndexGate {
    fn enter<'gate>(&'gate self, collection_id: &'gate str, _index_id: u32) -> GateWait<'gate> {
        Box::pin(async move {
            let entry = self.entries.fetch_add(1, Ordering::AcqRel) + 1;
            self.collections.insert(collection_id.to_owned(), ());
            // A watermark gate lets the first batch through, so the build
            // records a non-zero watermark before it is held.
            if self.kind == IndexGate::Watermark && entry == 1 {
                return;
            }
            if self.open.is_set() {
                return;
            }
            self.held.fetch_add(1, Ordering::AcqRel);
            // A build the scenario never opens the gate for (a node that
            // waits for it before the scenario's next step) is let through
            // after the limit and reported, so the scenario fails instead of
            // hanging.
            if workers::timeout(HOLD_LIMIT, self.open.wait())
                .await
                .is_err()
            {
                self.overheld.store(true, Ordering::Release);
            }
        })
    }
}

//! State every handle onto one runtime shares.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::platform::Rt;
use crate::stats::Stats;

pub(crate) struct Shared {
    pub(crate) rt: Rt,
    pub(crate) deadline: Duration,
    pub(crate) stats: Stats,
    stopped: AtomicBool,
}

impl Shared {
    pub(crate) fn new(rt: Rt, deadline: Duration) -> Arc<Self> {
        Arc::new(Self {
            rt,
            deadline,
            stats: Stats::default(),
            stopped: AtomicBool::new(false),
        })
    }

    /// The runtime is about to stop: from now on its timers report that
    /// instead of being polled against a driver that is going away. The
    /// browser's loop never stops, so only a native owner calls it.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

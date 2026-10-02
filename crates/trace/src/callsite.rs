//! Per-callsite enablement, modelled on `tracing`'s callsite caching.
//!
//! Each site owns a static whose decision is resolved once against the active
//! level and then read with a relaxed load. An epoch counter invalidates every
//! decision when the level changes, so sites re-resolve lazily on next use
//! rather than caching the first answer forever. No registry is required: the
//! epoch comparison is what makes a level change take effect.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// Verbosity of a span callsite, ordered so a lower value is more severe.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

const UNRESOLVED: u8 = 0;
const ENABLED: u8 = 1;
const DISABLED: u8 = 2;

static MAX_LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);
/// Bumped on every level change so resolved callsites know to re-check.
static EPOCH: AtomicUsize = AtomicUsize::new(1);

pub struct Callsite {
    level: u8,
    state: AtomicU8,
    epoch: AtomicUsize,
}

impl Callsite {
    pub const fn new(level: Level) -> Self {
        Self {
            level: level as u8,
            state: AtomicU8::new(UNRESOLVED),
            epoch: AtomicUsize::new(0),
        }
    }

    /// The hot path: one relaxed load once the decision is settled.
    #[inline(always)]
    pub fn enabled(&self) -> bool {
        let state = self.state.load(Ordering::Relaxed);
        if state != UNRESOLVED
            && self.epoch.load(Ordering::Relaxed) == EPOCH.load(Ordering::Relaxed)
        {
            return state == ENABLED;
        }
        self.resolve()
    }

    #[cold]
    #[inline(never)]
    fn resolve(&self) -> bool {
        let epoch = EPOCH.load(Ordering::Relaxed);
        let on = self.level <= MAX_LEVEL.load(Ordering::Relaxed);
        self.state
            .store(if on { ENABLED } else { DISABLED }, Ordering::Relaxed);
        self.epoch.store(epoch, Ordering::Relaxed);
        on
    }
}

/// Set the maximum enabled level. Every callsite re-resolves on next use.
pub fn set_max_level(level: Level) {
    MAX_LEVEL.store(level as u8, Ordering::Relaxed);
    EPOCH.fetch_add(1, Ordering::Relaxed);
}

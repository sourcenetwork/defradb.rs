//! Per-callsite enablement, modelled on `tracing`'s callsite caching.
//!
//! Each site owns a static whose decision is resolved once against the active
//! directives and then read with a relaxed load. An epoch counter invalidates
//! every decision when the directives change, so sites re-resolve lazily on
//! next use rather than caching the first answer forever. No registry is
//! required: the epoch comparison is what makes a change take effect.
//!
//! Directives use the `RUST_LOG` shape so one configuration string can drive
//! both `tracing` events and `fastrace` spans — two filter languages would be
//! worse than the single `EnvFilter` we had before the migration.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::RwLock;

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

impl Level {
    fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Level::Error),
            "warn" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "debug" => Some(Level::Debug),
            "trace" => Some(Level::Trace),
            _ => None,
        }
    }
}

const UNRESOLVED: u8 = 0;
const ENABLED: u8 = 1;
const DISABLED: u8 = 2;

static DEFAULT_LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);
/// Bumped on every directive change so resolved callsites know to re-check.
static EPOCH: AtomicUsize = AtomicUsize::new(1);
/// Target-scoped overrides, held most-specific-first so the first prefix match
/// wins.
static TARGETS: RwLock<Vec<(String, u8)>> = RwLock::new(Vec::new());

pub struct Callsite {
    target: &'static str,
    level: u8,
    state: AtomicU8,
    epoch: AtomicUsize,
}

impl Callsite {
    pub const fn new(target: &'static str, level: Level) -> Self {
        Self {
            target,
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
        let ceiling = TARGETS
            .read()
            .ok()
            .and_then(|targets| {
                targets
                    .iter()
                    .find(|(target, _)| self.target.starts_with(target.as_str()))
                    .map(|(_, level)| *level)
            })
            .unwrap_or_else(|| DEFAULT_LEVEL.load(Ordering::Relaxed));
        let on = self.level <= ceiling;
        self.state
            .store(if on { ENABLED } else { DISABLED }, Ordering::Relaxed);
        self.epoch.store(epoch, Ordering::Relaxed);
        on
    }
}

/// Set the maximum enabled level for callsites with no target override.
pub fn set_max_level(level: Level) {
    DEFAULT_LEVEL.store(level as u8, Ordering::Relaxed);
    EPOCH.fetch_add(1, Ordering::Relaxed);
}

/// Apply `RUST_LOG`-style directives, e.g.
/// `"info,iroh_quinn_proto::connection=error,p2p::sync=debug"`.
///
/// A bare level sets the default; `target=level` scopes one. Unparseable
/// entries are ignored rather than failing, matching `EnvFilter`'s tolerance —
/// a malformed directive should not stop a node from starting.
pub fn set_directives(spec: &str) {
    let mut targets: Vec<(String, u8)> = Vec::new();
    let mut default = None;

    for entry in spec.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        match entry.split_once('=') {
            Some((target, level)) => {
                if let Some(level) = Level::parse(level) {
                    targets.push((target.trim().to_owned(), level as u8));
                }
            }
            None => {
                if let Some(level) = Level::parse(entry) {
                    default = Some(level as u8);
                }
            }
        }
    }

    // Longest target first, so `p2p::sync::coordinator` beats `p2p`.
    targets.sort_by_key(|(target, _)| std::cmp::Reverse(target.len()));

    if let Ok(mut slot) = TARGETS.write() {
        *slot = targets;
    }
    if let Some(default) = default {
        DEFAULT_LEVEL.store(default, Ordering::Relaxed);
    }
    EPOCH.fetch_add(1, Ordering::Relaxed);
}

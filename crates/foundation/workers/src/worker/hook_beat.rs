//! The hook poll's side of the heartbeat. A hook poll right after a task
//! poll takes the heartbeat over from the task in one store; it stays shown
//! until the next task poll or the end of the turn replaces it, and is
//! accounted for then. When the monitor watches, the hook reads no clock:
//! the monitor times it from its first sighting.

use super::heartbeat::hook_word;
use super::local::Local;
use super::shared::bump;
use super::{micros, report_long_hook};

/// No open hook poll: no hook word is 0.
pub(super) const NO_HOOK: u64 = 0;

impl Local {
    /// The time, when nothing watches this worker and it times its own
    /// polls; 0, read from no clock, when the monitor does.
    #[inline]
    pub(super) fn own_now(&self) -> u64 {
        if self.tuning.monitored {
            0
        } else {
            self.now()
        }
    }

    /// A hook poll begins, at `start` on [`own_now`](Self::own_now).
    pub(super) fn begin_hook(&self, start: u64) {
        let number = self.hooks.get().wrapping_add(1);
        self.hooks.set(number);
        let word = hook_word(number);
        self.shared.heartbeat.begin_hook(word);
        self.open_hook.set(word);
        if !self.tuning.monitored {
            self.hook_start.set(start);
        }
    }

    /// The heartbeat has left the open hook poll: takes the monitor's mark
    /// for it off, or, when nothing watches, measures it as ending at `end`.
    pub(super) fn close_hook<F>(&self, end: F)
    where
        F: FnOnce() -> u64,
    {
        let word = self.open_hook.replace(NO_HOOK);
        if word == NO_HOOK || self.shared.unmark(word) || self.tuning.monitored {
            return;
        }
        let elapsed = end().saturating_sub(self.hook_start.get());
        if elapsed > micros(self.tuning.stuck_after) {
            bump(&self.shared.counters.long_hook_polls);
            report_long_hook(self.shared.id.index, elapsed, self.tuning.stuck_after);
        }
    }

    /// The worker is between polls: the heartbeat goes idle and an open hook
    /// poll is accounted for.
    pub(super) fn end_beat(&self) {
        self.shared.heartbeat.idle();
        self.close_hook(|| self.now());
    }
}

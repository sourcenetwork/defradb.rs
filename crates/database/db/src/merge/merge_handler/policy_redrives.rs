//! At most one re-drive per policy at a time (`proofs/tla/ForeignPolicy.tla`,
//! `moved`).
//!
//! A busy home reaches a new state with every block it finalizes, and each
//! one asks again about everything parked under its policies. While a
//! re-drive of a policy runs, the notices that arrive for it fold into one
//! more run after it, which reads the newest state; they never start a
//! second re-drive beside it, and none is lost.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;

use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;

/// No re-drive of the policy runs.
const IDLE: u8 = 0;
/// A re-drive runs, and nothing arrived since it began.
const RUNNING: u8 = 1;
/// A re-drive runs, and a notice arrived since it began: it runs once more.
const AGAIN: u8 = 2;

/// The state of each policy's re-drive. An entry is kept once made: there is
/// one per policy ever re-driven, which is one per policy routed elsewhere.
#[derive(Default)]
pub(crate) struct PolicyRedrives {
    states: HopscotchMap<String, Arc<PolicyState>, RandomState>,
}

/// One policy's re-drive: whether one runs, and how many parked objects its
/// runs have released so far.
#[derive(Default)]
struct PolicyState {
    run: AtomicU8,
    released: AtomicUsize,
}

/// The right to run a policy's re-drive, held by the one task running it.
/// A turn dropped before [`Turn::again`] ended it, by a task that stopped
/// early or panicked, ends there, so the policy's next notice starts a
/// re-drive again; a notice folded into it is left to the sweep.
pub(crate) struct Turn {
    state: Arc<PolicyState>,
    ended: bool,
}

impl PolicyRedrives {
    /// The turn to re-drive `policy`, or `None` when one is running: that
    /// one then runs once more.
    pub(crate) fn begin(&self, policy: &str) -> Option<Turn> {
        let state = self
            .states
            .get_or_insert(policy_key(policy), Arc::default());
        let mut current = state.run.load(Ordering::Acquire);
        loop {
            let next = if current == IDLE { RUNNING } else { AGAIN };
            match state
                .run
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(IDLE) => {
                    return Some(Turn {
                        state,
                        ended: false,
                    })
                }
                Ok(_) => return None,
                Err(seen) => current = seen,
            }
        }
    }

    /// How many parked objects of `policy` its re-drives have released.
    pub(crate) fn released(&self, policy: &str) -> usize {
        self.states
            .get(&policy_key(policy))
            .map_or(0, |state| state.released.load(Ordering::Acquire))
    }
}

impl Turn {
    /// Count `objects` this run released.
    pub(crate) fn released(&self, objects: usize) {
        self.state.released.fetch_add(objects, Ordering::AcqRel);
    }

    /// Whether the re-drive just run must run again because a notice arrived
    /// while it ran. `false` ends the turn.
    pub(crate) fn again(&mut self) -> bool {
        if self
            .state
            .run
            .compare_exchange(AGAIN, RUNNING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return true;
        }
        let again = self
            .state
            .run
            .compare_exchange(RUNNING, IDLE, Ordering::AcqRel, Ordering::Acquire)
            .is_err();
        self.ended = !again;
        again
    }
}

impl Drop for Turn {
    /// A turn ended by [`Turn::again`] is not touched again: the policy's
    /// state may already belong to the next turn.
    fn drop(&mut self) {
        if !self.ended {
            self.state.run.store(IDLE, Ordering::Release);
        }
    }
}

/// A policy identifier as one key, however its hex is spelled.
fn policy_key(policy: &str) -> String {
    policy
        .strip_prefix("0x")
        .or_else(|| policy.strip_prefix("0X"))
        .unwrap_or(policy)
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Notices that arrive while a policy's re-drive runs start no second
    /// one: they fold into one more run, after which the turn ends and the
    /// next notice starts a new one.
    #[test]
    fn notices_during_a_redrive_fold_into_one_more_run() {
        let redrives = PolicyRedrives::default();
        let mut turn = redrives.begin("0xAB").expect("the first notice runs");
        assert!(redrives.begin("ab").is_none());
        assert!(redrives.begin("AB").is_none());
        assert!(
            redrives.begin("other").is_some(),
            "another policy runs apart"
        );
        turn.released(2);
        assert_eq!(redrives.released("0XAB"), 2);
        assert!(turn.again(), "the folded notices run once more");
        assert!(!turn.again(), "nothing arrived during that run");
        let next = redrives.begin("ab").expect("the next notice starts a turn");
        drop(turn);
        assert!(
            redrives.begin("ab").is_none(),
            "an ended turn dropped leaves the next one running"
        );
        drop(next);
    }

    /// A turn dropped before it ended, by a task that stopped early or
    /// panicked, ends: the policy's next notice starts a re-drive.
    #[test]
    fn a_turn_dropped_before_it_ended_lets_the_next_notice_run() {
        let redrives = PolicyRedrives::default();
        let turn = redrives.begin("ab").expect("the first notice runs");
        assert!(redrives.begin("ab").is_none(), "a notice folds in");
        drop(turn);
        let mut next = redrives.begin("ab").expect("the next notice starts a turn");
        assert!(redrives.begin("ab").is_none());
        assert!(next.again(), "the folded notice runs once more");
        assert!(!next.again());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _turn = redrives.begin("ab").expect("a turn");
            panic!("the re-drive panicked");
        }))
        .is_err());
        assert!(
            redrives.begin("ab").is_some(),
            "a turn its task panicked in ends too"
        );
    }
}

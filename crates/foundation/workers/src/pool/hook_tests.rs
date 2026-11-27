//! The I/O hook's time against the stuck-poll monitor: a slow hook poll after
//! a short task is the hook's, never the task's.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use super::tests::bare;
use super::{monitor, PoolShared};
use crate::clock::{Clock, ManualClock};
use crate::config::Config;
use crate::hook::{Hook, HookProgress};
use crate::worker::{hook_word, task_word, Worker, WorkerStats, REPORTED};

/// A hook whose first poll moves the clock past the stuck limit, as a slow
/// block fetch would, and, when the worker is watched, runs the monitor's
/// scan as it starts and again while it is in there.
struct SlowOnce {
    pool: Arc<PoolShared>,
    clock: Arc<ManualClock>,
    scan: bool,
    slow: bool,
}

impl Hook for SlowOnce {
    fn poll(&mut self, _budget: usize) -> HookProgress {
        if std::mem::take(&mut self.slow) {
            if self.scan {
                assert_eq!(
                    monitor::scan(&self.pool, self.clock.now_micros()),
                    0,
                    "first seen now"
                );
            }
            self.clock.advance(50_000);
            if self.scan {
                assert_eq!(monitor::scan(&self.pool, self.clock.now_micros()), 1);
            }
        }
        HookProgress::default()
    }

    fn idle(&mut self, _waker: &Waker) {}
}

/// A task that returns at once from each of three polls, recording whether
/// each poll carried the reported mark. With `late_mark`, its second poll
/// raises the monitor's mark for the poll that started at 0, long ended, as
/// a monitor preempted between its read and its mark would.
struct Short {
    pool: Arc<PoolShared>,
    polls: u32,
    late_mark: bool,
    seen_reported: Rc<RefCell<Vec<bool>>>,
}

impl Future for Short {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let reported = self.pool.workers[0]
            .heartbeat
            .read()
            .is_some_and(|beat| beat.task & REPORTED != 0);
        self.seen_reported.borrow_mut().push(reported);
        self.polls += 1;
        if self.late_mark && self.polls == 2 {
            self.pool.workers[0].mark_stuck(task_word(0));
        }
        if self.polls == 3 {
            return Poll::Ready(());
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// Runs a short three-poll task beside a hook whose first poll is slow; the
/// worker's stats after, and whether each task poll carried the reported
/// mark.
fn run(monitored: bool) -> (WorkerStats, Vec<bool>, bool) {
    let clock = Arc::new(ManualClock::default());
    let pool = bare(2, clock.clone());
    let hook = SlowOnce {
        pool: pool.clone(),
        clock: clock.clone(),
        scan: monitored,
        slow: true,
    };
    let mut worker = Worker::new(
        pool.workers[0].clone(),
        clock,
        PoolShared::tuning(&Config::default(), monitored),
        Some(Box::new(hook)),
    )
    .unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _done = worker
        .spawn(Short {
            pool: pool.clone(),
            polls: 0,
            late_mark: false,
            seen_reported: seen.clone(),
        })
        .unwrap();
    while worker.turn() > 0 {}
    let stuck = pool.workers[0].is_stuck();
    let seen = seen.borrow().clone();
    (pool.workers[0].stats(), seen, stuck)
}

#[test]
fn a_watched_slow_hook_after_a_short_task_is_the_hooks_not_the_tasks() {
    let (stats, seen, stuck) = run(true);
    assert_eq!(stats.long_polls, 0, "no task poll ran past the limit");
    assert_eq!(stats.long_hook_polls, 1, "the hook poll did");
    assert_eq!(seen, vec![false; 3], "the task is never marked");
    assert!(!stuck, "the hook's mark is cleared when it returns");
}

#[test]
fn an_unwatched_slow_hook_after_a_short_task_is_the_hooks_not_the_tasks() {
    let (stats, seen, stuck) = run(false);
    assert_eq!(stats.long_polls, 0);
    assert_eq!(stats.long_hook_polls, 1);
    assert_eq!(seen, vec![false; 3]);
    assert!(!stuck);
}

/// Runs one turn of a worker with no task, so the turn polls the hook on
/// its own, and that hook poll is slow; the worker's stats after, and
/// whether it is left marked.
fn run_hook_alone(monitored: bool) -> (WorkerStats, bool) {
    let clock = Arc::new(ManualClock::default());
    let pool = bare(2, clock.clone());
    let hook = SlowOnce {
        pool: pool.clone(),
        clock: clock.clone(),
        scan: monitored,
        slow: true,
    };
    let mut worker = Worker::new(
        pool.workers[0].clone(),
        clock,
        PoolShared::tuning(&Config::default(), monitored),
        Some(Box::new(hook)),
    )
    .unwrap();
    worker.turn();
    (pool.workers[0].stats(), pool.workers[0].is_stuck())
}

#[test]
fn a_watched_slow_hook_with_no_task_before_it_is_found() {
    let (stats, stuck) = run_hook_alone(true);
    assert_eq!(stats.long_hook_polls, 1, "the scan saw the hook poll");
    assert_eq!(stats.long_polls, 0);
    assert!(!stuck, "the hook's mark is cleared when the turn ends");
}

#[test]
fn an_unwatched_slow_hook_with_no_task_before_it_is_found() {
    let (stats, stuck) = run_hook_alone(false);
    assert_eq!(
        stats.long_hook_polls, 1,
        "the worker timed its own hook poll"
    );
    assert_eq!(stats.long_polls, 0);
    assert!(!stuck);
}

#[test]
fn a_held_up_monitor_does_not_replace_the_mark_of_the_poll_shown_now() {
    let clock = Arc::new(ManualClock::default());
    let pool = bare(2, clock.clone());
    let worker = &pool.workers[0];
    let (older, newer) = (task_word(0), hook_word(1));
    worker.heartbeat.begin_hook(newer);
    worker.mark_stuck(newer);
    worker.mark_stuck(older);
    assert!(
        !worker.unmark(older),
        "the held-up monitor's recheck finds no mark of its own"
    );
    assert!(worker.is_stuck(), "the poll shown now stays marked");
    assert!(worker.unmark(newer));
}

#[test]
fn a_mark_for_a_poll_no_longer_shown_is_replaced() {
    let clock = Arc::new(ManualClock::default());
    let pool = bare(2, clock.clone());
    let worker = &pool.workers[0];
    worker.mark_stuck(task_word(0));
    worker.heartbeat.begin_hook(hook_word(1));
    worker.mark_stuck(hook_word(1));
    assert!(worker.unmark(hook_word(1)), "the shown poll's mark landed");
    assert!(!worker.is_stuck());
}

#[test]
fn a_held_up_monitor_does_not_report_an_older_hook_poll_again() {
    let clock = Arc::new(ManualClock::default());
    let pool = bare(2, clock.clone());
    let worker = &pool.workers[0];
    let (first, second) = (hook_word(1), hook_word(2));
    assert!(
        worker.claim_hook_report(first),
        "the first sighting reports"
    );
    assert!(
        !worker.claim_hook_report(first),
        "the other monitor does not"
    );
    assert!(
        worker.claim_hook_report(second),
        "a newer hook poll reports"
    );
    assert!(
        !worker.claim_hook_report(first),
        "a monitor held up since it read the first poll does not report it again"
    );
    assert!(!worker.claim_hook_report(second));
}

#[test]
fn a_held_up_monitor_does_not_report_an_older_task_poll_again() {
    let clock = Arc::new(ManualClock::default());
    let pool = bare(2, clock);
    let worker = &pool.workers[0];
    let (first, second) = (task_word(0), task_word(1));
    assert!(
        worker.claim_task_report(first),
        "the first task poll reports, including word zero"
    );
    assert!(
        !worker.claim_task_report(first),
        "the other monitor does not"
    );
    assert!(
        worker.claim_task_report(second),
        "a newer task poll reports"
    );
    assert!(
        !worker.claim_task_report(first),
        "a monitor held up since it read the first poll does not report it again"
    );
    assert!(!worker.claim_task_report(second));
}

#[test]
fn the_monitor_reports_a_hook_poll_once_and_routes_around_it() {
    let clock = Arc::new(ManualClock::default());
    let pool = bare(2, clock.clone());
    let worker = &pool.workers[0];
    worker.heartbeat.begin_hook(hook_word(1));
    assert_eq!(monitor::scan(&pool, clock.now_micros()), 0, "first seen");
    clock.advance(9_000);
    assert_eq!(
        monitor::scan(&pool, clock.now_micros()),
        0,
        "within the limit"
    );
    clock.advance(11_000);
    assert_eq!(monitor::scan(&pool, clock.now_micros()), 1);
    monitor::scan(&pool, clock.now_micros());
    assert_eq!(
        worker.stats().long_hook_polls,
        1,
        "reported once per hook poll"
    );
    assert_eq!(worker.stats().long_polls, 0);
    assert!(worker.is_stuck(), "routed around while slow");
    worker.heartbeat.begin_hook(hook_word(2));
    assert_eq!(
        monitor::scan(&pool, clock.now_micros()),
        0,
        "a new hook poll"
    );
    assert!(!worker.is_stuck(), "the old poll's mark is swept");
    clock.advance(20_000);
    monitor::scan(&pool, clock.now_micros());
    assert_eq!(
        worker.stats().long_hook_polls,
        2,
        "a later slow hook poll is new"
    );
}

#[test]
fn a_scan_sweeps_a_mark_for_a_poll_no_longer_shown() {
    let clock = Arc::new(ManualClock::default());
    let pool = bare(2, clock.clone());
    let worker = &pool.workers[0];
    worker.mark_stuck(task_word(0));
    assert!(worker.is_stuck());
    assert_eq!(monitor::scan(&pool, clock.now_micros()), 0);
    assert!(!worker.is_stuck(), "an idle worker is not left marked");
}

/// A hook whose every poll moves the clock a microsecond on, so each task
/// poll starts at its own time.
struct Tick {
    clock: Arc<ManualClock>,
}

impl Hook for Tick {
    fn poll(&mut self, _budget: usize) -> HookProgress {
        self.clock.advance(1);
        HookProgress::default()
    }

    fn idle(&mut self, _waker: &Waker) {}
}

#[test]
fn a_late_mark_for_an_ended_poll_is_not_charged_to_the_next_one() {
    let clock = Arc::new(ManualClock::default());
    let pool = bare(2, clock.clone());
    let mut worker = Worker::new(
        pool.workers[0].clone(),
        clock.clone(),
        PoolShared::tuning(&Config::default(), true),
        Some(Box::new(Tick { clock })),
    )
    .unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _done = worker
        .spawn(Short {
            pool: pool.clone(),
            polls: 0,
            late_mark: true,
            seen_reported: seen.clone(),
        })
        .unwrap();
    while worker.turn() > 0 {}
    assert_eq!(*seen.borrow(), vec![false; 3], "the task is never marked");
    assert!(
        pool.workers[0].unmark(task_word(0)),
        "the stale mark waits for the monitor's recheck, untouched"
    );
}

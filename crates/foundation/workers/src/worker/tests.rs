//! The executor driven turn by turn on the test thread, so every case is
//! deterministic: spawning, wakes from other threads, panics, per-key order,
//! timers on a manual clock, yields, shutdown and self-wait detection.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use regolith::sync::Event;

use super::*;
use crate::clock::ManualClock;

fn tuning(monitored: bool) -> Tuning {
    Tuning {
        inbox_batch: 8,
        run_batch: 8,
        io_budget: 8,
        idle_spin: Duration::ZERO,
        stuck_after: Duration::from_millis(10),
        monitored,
    }
}

fn shared() -> Arc<WorkerShared> {
    let signal = Arc::new(Signal::new(Parker::Waker(Waker::noop().clone())));
    Arc::new(WorkerShared::new(
        WorkerId { index: 0, node: 0 },
        signal,
        4,
        1 << 20,
    ))
}

fn worker_with(clock: Arc<ManualClock>, monitored: bool) -> (Worker, Arc<WorkerShared>) {
    let shared = shared();
    let worker = Worker::new(shared.clone(), clock, tuning(monitored), None).unwrap();
    (worker, shared)
}

fn worker() -> (Worker, Arc<WorkerShared>) {
    worker_with(Arc::new(ManualClock::default()), true)
}

fn settle(worker: &mut Worker) {
    while worker.turn() > 0 {}
}

/// Polls a completion once with a no-op waker.
fn ready<T>(completion: &mut Completion<T>) -> Option<Result<T, Error>> {
    match Pin::new(completion).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

fn job<F, Fut>(shared: &Arc<WorkerShared>, key: Option<u64>, make: F) -> Completion<Fut::Output>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future + 'static,
    Fut::Output: Send + 'static,
{
    let admitted = shared.try_admit(1).expect("room");
    let (job, completion) = Job::new(make, "test", key, false, admitted);
    shared.push(job);
    completion
}

#[test]
fn a_local_task_runs_to_its_completion_on_this_thread() {
    let (mut worker, _) = worker();
    assert!(on_worker());
    assert_eq!(current_worker(), Some(WorkerId { index: 0, node: 0 }));
    let not_send = Rc::new(41);
    let mut done = spawn_local(async move { *not_send + 1 }).unwrap();
    assert_eq!(ready(&mut done), None);
    settle(&mut worker);
    assert_eq!(ready(&mut done), Some(Ok(42)));
    drop(worker);
    assert!(!on_worker());
    assert_eq!(spawn_local(async {}).err(), Some(Error::NotOnWorker));
    assert!(sleep(Duration::from_secs(1)).is_err());
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn a_blocking_wait_on_a_worker_is_an_error_not_a_deadlock() {
    let (worker, _) = worker();
    let (sender, completion) = crate::completion::channel::<u8>();
    assert_eq!(
        crate::completion::block_on(async { 1 }),
        Err(Error::OnWorker)
    );
    assert_eq!(completion.wait(), Err(Error::OnWorker));
    drop(sender);
    drop(worker);
    assert_eq!(
        crate::completion::block_on(async { 1 }),
        Ok(1),
        "a plain thread again"
    );
}

#[test]
fn a_wake_from_another_thread_puts_the_task_back_on_this_worker() {
    let (mut worker, shared) = worker();
    let event = Arc::new(Event::new());
    let waited = event.clone();
    let mut done = spawn_local(async move { waited.wait().await }).unwrap();
    settle(&mut worker);
    assert_eq!(ready(&mut done), None);
    assert!(!shared.signal.has_ready());
    std::thread::spawn(move || event.set()).join().unwrap();
    assert!(shared.signal.has_ready());
    settle(&mut worker);
    assert_eq!(ready(&mut done), Some(Ok(())));
}

#[test]
fn a_panicking_task_fails_only_its_own_completion() {
    let (mut worker, shared) = worker();
    let mut boom = spawn_local(async { panic!("boom") }).unwrap();
    let mut fine = spawn_local(async { 7 }).unwrap();
    let mut submitted = job(&shared, None, || async { panic!("in make") });
    settle(&mut worker);
    match ready(&mut boom) {
        Some(Err(Error::Panicked { message, .. })) => assert_eq!(message, "boom"),
        other => panic!("{other:?}"),
    }
    assert_eq!(ready(&mut fine), Some(Ok(7)));
    assert!(matches!(
        ready(&mut submitted),
        Some(Err(Error::Panicked { .. }))
    ));
    assert_eq!(shared.stats().panics, 2);
    assert_eq!(shared.load(), 0, "the panicked job gave its admission back");
}

#[test]
fn one_task_per_key_runs_and_the_next_starts_when_it_finishes() {
    let (mut worker, shared) = worker();
    let gate = Arc::new(Event::new());
    let second_started = Arc::new(AtomicBool::new(false));
    let first = {
        let gate = gate.clone();
        job(&shared, Some(9), move || async move { gate.wait().await })
    };
    let second = {
        let started = second_started.clone();
        job(&shared, Some(9), move || async move {
            started.store(true, Ordering::SeqCst)
        })
    };
    let mut other_key = job(&shared, Some(10), || async { 3 });
    settle(&mut worker);
    assert!(
        !second_started.load(Ordering::SeqCst),
        "waits behind key 9's running task"
    );
    assert_eq!(ready(&mut other_key), Some(Ok(3)));
    gate.set();
    settle(&mut worker);
    assert!(second_started.load(Ordering::SeqCst));
    drop((first, second));
}

#[test]
fn a_timer_fires_when_the_clock_reaches_it() {
    let clock = Arc::new(ManualClock::default());
    let (mut worker, _) = worker_with(clock.clone(), true);
    let mut slept = spawn_local(async {
        sleep(Duration::from_millis(10)).unwrap().await;
    })
    .unwrap();
    settle(&mut worker);
    assert_eq!(worker.next_due(), Some(10_000));
    clock.advance(9_999);
    settle(&mut worker);
    assert_eq!(ready(&mut slept), None);
    clock.advance(1);
    settle(&mut worker);
    assert_eq!(ready(&mut slept), Some(Ok(())));
    assert_eq!(worker.next_due(), None);
}

#[test]
fn periodic_jobs_run_once_per_period() {
    let clock = Arc::new(ManualClock::default());
    let (mut worker, _) = worker_with(clock.clone(), true);
    let runs = Rc::new(RefCell::new(0));
    let counted = runs.clone();
    spawn_daemon(async move {
        let _ = every(Duration::from_millis(5), || {
            *counted.borrow_mut() += 1;
            async {}
        })
        .await;
    })
    .unwrap();
    for _ in 0..3 {
        settle(&mut worker);
        clock.advance(5_000);
    }
    settle(&mut worker);
    assert_eq!(*runs.borrow(), 3);
}

#[test]
fn yield_now_lets_the_other_ready_tasks_run_first() {
    let (mut worker, _) = worker();
    let order = Rc::new(RefCell::new(Vec::new()));
    let a = order.clone();
    let b = order.clone();
    let _first = spawn_local(async move {
        a.borrow_mut().push("a1");
        yield_now().await;
        a.borrow_mut().push("a2");
    })
    .unwrap();
    let _second = spawn_local(async move { b.borrow_mut().push("b") }).unwrap();
    settle(&mut worker);
    assert_eq!(*order.borrow(), vec!["a1", "b", "a2"]);
}

#[test]
fn shutdown_finishes_admitted_work_but_not_daemons() {
    let (mut worker, shared) = worker();
    spawn_daemon(std::future::pending()).unwrap();
    let mut done = job(&shared, None, || async { 5 });
    shared.shutdown();
    let mut turns = 0;
    while !worker.finished() {
        worker.turn();
        turns += 1;
        assert!(turns < 100, "shutdown never finished");
    }
    assert_eq!(ready(&mut done), Some(Ok(5)));
}

#[test]
fn stopping_fails_unfinished_work_and_gives_admission_back() {
    let (mut worker, shared) = worker();
    let mut pending = job(&shared, None, std::future::pending::<()>);
    let mut keyed = job(&shared, Some(1), std::future::pending::<()>);
    let mut behind = job(&shared, Some(1), || async {});
    settle(&mut worker);
    assert_eq!(shared.load(), 3);
    drop(worker);
    assert_eq!(ready(&mut pending), Some(Err(Error::ShutDown)));
    assert_eq!(ready(&mut keyed), Some(Err(Error::ShutDown)));
    assert_eq!(ready(&mut behind), Some(Err(Error::ShutDown)));
    assert_eq!(shared.load(), 0);
    let mut late = job(&shared, None, || async {});
    assert_eq!(
        ready(&mut late),
        Some(Err(Error::ShutDown)),
        "a push after exit is failed at once"
    );
    assert_eq!(shared.load(), 0);
}

#[test]
fn abandon_stops_at_once() {
    let (mut worker, shared) = worker();
    let _pending = job(&shared, None, std::future::pending::<()>);
    settle(&mut worker);
    assert!(!worker.finished());
    shared.abandon();
    assert!(worker.finished());
}

/// A future that moves the manual clock forward inside its poll, as a
/// blocking call would, and records whether its poll was marked reported.
struct Blocks {
    clock: Arc<ManualClock>,
    shared: Arc<WorkerShared>,
    polls: u32,
    seen_reported: Rc<RefCell<Vec<bool>>>,
}

impl Future for Blocks {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let reported = self
            .shared
            .heartbeat
            .read()
            .is_some_and(|beat| beat.task & REPORTED != 0);
        self.seen_reported.borrow_mut().push(reported);
        self.clock.advance(20_000);
        self.polls += 1;
        if self.polls == 2 {
            return Poll::Ready(());
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[test]
fn an_unwatched_worker_measures_its_own_long_polls_and_reports_a_task_once() {
    let clock = Arc::new(ManualClock::default());
    let (mut worker, shared) = worker_with(clock.clone(), false);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _done = spawn_local(Blocks {
        clock,
        shared: shared.clone(),
        polls: 0,
        seen_reported: seen.clone(),
    });
    settle(&mut worker);
    assert_eq!(shared.stats().long_polls, 2);
    assert_eq!(
        *seen.borrow(),
        vec![false, true],
        "the second poll carries the reported mark"
    );
}

#[test]
fn a_poll_the_monitor_flagged_is_cleared_when_it_returns() {
    let (mut worker, shared) = worker();
    let flagger = shared.clone();
    let _done = spawn_local(async move { flagger.stuck.store(true, Ordering::SeqCst) });
    settle(&mut worker);
    assert!(!shared.stuck.load(Ordering::SeqCst));
}

#[test]
fn a_second_worker_on_one_thread_is_refused() {
    let (_worker, shared) = worker();
    assert_eq!(
        Worker::new(shared, Arc::new(ManualClock::default()), tuning(true), None).err(),
        Some(Error::OnWorker)
    );
}

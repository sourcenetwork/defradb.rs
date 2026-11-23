//! The pool's API for the data path: spawning from anywhere, background work,
//! deadlines a `Send` future can carry, a monitor driven from outside, and
//! each worker's context switches.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use regolith::sync::SegQueue;

use crate::{Config, Error, Pool, Target};

const DEADLINE: Duration = Duration::from_secs(60);

fn pool(workers: usize) -> Pool {
    Pool::start(Config::unplaced(workers)).unwrap()
}

#[test]
fn spawn_runs_on_the_calling_worker_or_on_the_named_pool() {
    let pool = pool(2);
    let off = crate::spawn(Some(&pool), "off", || async { crate::current_worker() })
        .unwrap()
        .wait_timeout(DEADLINE)
        .unwrap();
    assert!(off.is_some(), "off a worker it runs on the named pool");

    let (outer, inner) = pool
        .submit_and_wait(Target::Worker(1), || async {
            let inner = crate::spawn(None, "on", || async { crate::current_worker() })
                .unwrap()
                .await
                .unwrap();
            (crate::current_worker(), inner)
        })
        .unwrap();
    assert_eq!(outer, inner, "on a worker it stays on that worker");
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_non_waiting_submission_is_refused_when_the_worker_is_full() {
    let pool = Pool::start(Config {
        inbox_items: 1,
        ..Config::unplaced(1)
    })
    .unwrap();
    let release = Arc::new(regolith::sync::Event::new());
    let held = Arc::clone(&release);
    let first = crate::block_on(pool.submit(Target::Worker(0), move || async move {
        held.wait().await;
    }))
    .unwrap()
    .unwrap();
    let refused = pool.try_submit_with(Target::Worker(0), crate::Submit::default(), || async {});
    assert!(matches!(refused, Err(Error::Full)));
    release.set();
    first.wait_timeout(DEADLINE).unwrap();
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_timeout_ends_a_wait_that_runs_past_it_and_passes_one_that_does_not() {
    let pool = pool(1);
    let (late, early) = pool
        .submit_and_wait(Target::Worker(0), || async {
            let late = crate::timeout(Duration::from_millis(5), std::future::pending::<()>()).await;
            let early = crate::timeout(DEADLINE, async { 7 }).await;
            (late, early)
        })
        .unwrap();
    assert_eq!(late, Err(Error::TimedOut));
    assert_eq!(early, Ok(7));
    // Off every worker the timer is the fallback pool's.
    assert_eq!(
        crate::block_on(crate::timeout(
            Duration::from_millis(5),
            std::future::pending::<()>()
        ))
        .unwrap(),
        Err(Error::TimedOut)
    );
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_met_deadline_leaves_no_timer_behind() {
    let pool = pool(1);
    let cleared = pool
        .submit_and_wait(Target::Worker(0), || async {
            // The worker's own timers, such as its periodic recheck.
            let own = crate::worker::pending_timers();
            for _ in 0..1_000 {
                assert_eq!(crate::timeout(DEADLINE, async { 7 }).await, Ok(7));
            }
            // Yielding lets every timer task run: one that outlived its
            // deadline sets its timer then, one that ended does not.
            for _ in 0..100 {
                crate::yield_now().await;
            }
            crate::worker::pending_timers() <= own
        })
        .unwrap();
    assert!(cleared, "met deadlines still hold their timers");
    pool.shutdown_blocking().unwrap();
}

#[test]
fn met_deadlines_off_the_pool_never_fill_the_fallback_pool() {
    // More than the fallback pool admits at once: a timer that outlived its
    // deadline would hold the next one up until it fired.
    let started = Instant::now();
    for _ in 0..2 * Config::default().inbox_items + 1 {
        assert_eq!(
            crate::block_on(crate::timeout(DEADLINE, async { 7 })).unwrap(),
            Ok(7)
        );
    }
    assert!(
        started.elapsed() < DEADLINE / 2,
        "a met deadline waited for room"
    );
}

#[test]
fn background_work_stops_when_aborted_and_does_not_hold_up_shutdown() {
    let pool = pool(1);
    let ticks = Arc::new(SegQueue::new());
    let seen = Arc::clone(&ticks);
    let work = crate::spawn_background(Some(&pool), "ticks", move || async move {
        let _ = crate::every(Duration::from_millis(1), || {
            seen.push(());
            async {}
        })
        .await;
    });
    let started = Instant::now();
    while ticks.is_empty() {
        assert!(
            started.elapsed() < DEADLINE,
            "the background work never ran"
        );
        std::thread::yield_now();
    }
    work.abort();
    assert_eq!(crate::block_on_timeout(work, DEADLINE).unwrap(), Ok(()));

    let forever = crate::spawn_background(Some(&pool), "forever", std::future::pending);
    pool.shutdown_blocking().unwrap();
    assert!(crate::block_on_timeout(forever, DEADLINE).unwrap().is_err());

    let refused = crate::spawn_background(Some(&pool), "late", || async {});
    assert!(refused.is_finished());
    assert_eq!(crate::block_on(refused).unwrap(), Err(Error::ShutDown));
}

#[test]
fn a_monitor_driven_from_outside_finds_a_stuck_worker_and_ends_with_the_pool() {
    let monitors = Arc::new(SegQueue::new());
    let given = Arc::clone(&monitors);
    let pool = Pool::start(Config {
        stuck_after: Duration::from_millis(5),
        monitor: Some(Arc::new(move |monitor: crate::Monitor| given.push(monitor))),
        ..Config::unplaced(2)
    })
    .unwrap();
    let monitor = monitors
        .pop()
        .expect("the driver is handed the monitor at start");
    assert_eq!(monitor.scan(), Some(0));

    let release = Arc::new(AtomicBool::new(false));
    let held = Arc::clone(&release);
    let stuck = crate::block_on(pool.submit(Target::Worker(0), move || async move {
        // A poll that never yields: what the monitor exists to find.
        while !held.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
    }))
    .unwrap()
    .unwrap();
    let started = Instant::now();
    while monitor.scan() != Some(1) {
        assert!(
            started.elapsed() < DEADLINE,
            "the stuck worker was never found"
        );
        std::thread::yield_now();
    }
    release.store(true, Ordering::Release);
    stuck.wait_timeout(DEADLINE).unwrap();
    assert!(pool.stats().workers[0].long_polls >= 1);

    pool.shutdown_blocking().unwrap();
    assert_eq!(
        monitor.scan(),
        None,
        "the driver stops once the pool has stopped"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn each_workers_context_switches_are_counted() {
    let pool = pool(1);
    // An idle worker spins and need not switch at all: a sleep on it makes
    // one switch the count must show.
    pool.submit_and_wait(Target::Worker(0), || async {
        std::thread::sleep(Duration::from_millis(1));
    })
    .unwrap();
    let switches = pool.stats().workers[0]
        .switches
        .expect("Linux reports each thread's context switches");
    assert!(switches.voluntary + switches.involuntary > 0);
    pool.shutdown_blocking().unwrap();
}

#[test]
fn stop_drains_then_answers_and_refuses_on_a_worker() {
    let pool = pool(1);
    let inner = pool.clone();
    let on_worker = pool
        .submit_and_wait(Target::Worker(0), move || async move { inner.stop().await })
        .unwrap();
    assert_eq!(on_worker, Err(Error::OnWorker));
    assert_eq!(crate::block_on(pool.stop()).unwrap(), Ok(()));
}

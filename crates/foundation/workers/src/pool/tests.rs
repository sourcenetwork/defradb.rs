//! Routing and the stuck-poll monitor against a pool with no threads and a
//! manual clock; then real pools: targets, admission, containment, self-wait
//! detection, the blocking pool, one-worker modes, shutdown and pinning.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

use regolith::sync::Event;

use super::*;
use crate::clock::ManualClock;
use crate::config::{MemoryPolicy, Placement, Workers};
use crate::hook::{Hook, HookProgress};
use crate::topology::CpuSet;
use crate::worker::REPORTED;
use crate::{block_on, current_worker, placement};

/// Shared state of `workers` unpinned workers on one node, no threads.
fn bare(workers: usize, clock: Arc<ManualClock>) -> Arc<PoolShared> {
    let config = Config {
        workers: Workers::Count(workers),
        placement: Placement::Unpinned,
        inbox_items: 4,
        ..Config::default()
    };
    let topology = Topology::single(workers);
    let plan = placement::plan(&topology, &config);
    let placer = Arc::new(Placer::new(
        plan,
        CpuSet::new(),
        config.priorities.clone(),
        MemoryPolicy::Off,
    ));
    let parkers = (0..workers)
        .map(|_| Parker::Waker(Waker::noop().clone()))
        .collect();
    Arc::new(PoolShared::new(&config, &topology, placer, clock, parkers))
}

/// Queues a do-nothing job on `worker` as if `target` had routed it there.
fn queue(shared: &PoolShared, worker: usize, target: Target) -> Completion<()> {
    let worker = &shared.workers[worker];
    let admitted = worker.try_admit(1).expect("room");
    let key = match target {
        Target::Key(key) => Some(key),
        _ => None,
    };
    let (job, completion) = Job::new(
        || async {},
        "queued",
        key,
        matches!(target, Target::Worker(_)),
        admitted,
    );
    worker.push(job);
    completion
}

/// A small real pool: `workers` workers, a short idle spin, one blocking
/// thread.
fn small(workers: usize) -> Config {
    Config {
        workers: Workers::Count(workers),
        idle_spin: Duration::from_micros(10),
        blocking_threads: 1,
        ..Config::default()
    }
}

fn poll_once<F>(future: Pin<&mut F>) -> Poll<F::Output>
where
    F: Future + ?Sized,
{
    future.poll(&mut Context::from_waker(Waker::noop()))
}

/// A key whose home worker is `worker` among `shared`'s workers.
fn key_on(shared: &PoolShared, worker: usize) -> u64 {
    (1..)
        .map(|k: u64| k.wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .find(|key| shared.key_worker(*key).id.index == worker)
        .unwrap_or(0)
}

#[test]
fn the_monitor_reports_a_stuck_poll_once_and_routes_around_it() {
    let clock = Arc::new(ManualClock::default());
    let shared = bare(2, clock.clone());
    let stuck = &shared.workers[0];
    stuck.heartbeat.begin(0, 5, "blocker");
    clock.advance(9_000);
    assert_eq!(
        monitor::scan(&shared, clock.now_micros()),
        0,
        "within the 10 ms limit"
    );
    clock.advance(2_000);
    assert_eq!(monitor::scan(&shared, clock.now_micros()), 1);
    assert!(stuck.stuck.load(Ordering::SeqCst));
    assert_eq!(stuck.stats().long_polls, 1);
    monitor::scan(&shared, clock.now_micros());
    assert_eq!(stuck.stats().long_polls, 1, "reported once per task");
    for _ in 0..50 {
        assert_eq!(shared.route(Target::LeastLoaded).unwrap().id.index, 1);
    }
    assert_eq!(
        shared.route(Target::Worker(0)).unwrap().id.index,
        0,
        "the owner still gets its work"
    );
}

#[test]
fn an_idle_worker_is_never_marked_stuck() {
    let clock = Arc::new(ManualClock::default());
    let shared = bare(2, clock.clone());
    shared.workers[0].heartbeat.begin(0, 5, "short");
    shared.workers[0].heartbeat.idle();
    clock.advance(20_000);
    assert_eq!(monitor::scan(&shared, clock.now_micros()), 0);
    assert!(!shared.workers[0].stuck.load(Ordering::SeqCst));
}

#[test]
fn a_task_reported_before_is_not_reported_again() {
    let clock = Arc::new(ManualClock::default());
    let shared = bare(2, clock.clone());
    shared.workers[1].heartbeat.begin(0, 7 | REPORTED, "again");
    clock.advance(50_000);
    assert_eq!(monitor::scan(&shared, clock.now_micros()), 1);
    assert_eq!(shared.workers[1].stats().long_polls, 0);
    assert!(
        shared.workers[1].stuck.load(Ordering::SeqCst),
        "still routed around"
    );
}

#[test]
fn a_stuck_workers_movable_work_moves_and_its_bound_work_stays() {
    let clock = Arc::new(ManualClock::default());
    let shared = bare(2, clock.clone());
    let key = key_on(&shared, 0);
    let free = queue(&shared, 0, Target::LeastLoaded);
    let keyed = queue(&shared, 0, Target::Key(key));
    let bound = queue(&shared, 0, Target::Worker(0));
    shared.workers[0].heartbeat.begin(0, 1, "blocker");
    clock.advance(20_000);
    monitor::scan(&shared, clock.now_micros());
    assert_eq!(shared.workers[0].stats().moved_out, 2);
    assert_eq!(shared.workers[0].load(), 1, "the bound job keeps its place");
    assert_eq!(shared.workers[1].load(), 2);
    drop((free, keyed, bound));
}

#[test]
fn moved_work_stays_put_when_no_peer_has_room() {
    let clock = Arc::new(ManualClock::default());
    let shared = bare(2, clock.clone());
    let full: Vec<_> = (0..4)
        .map(|_| queue(&shared, 1, Target::LeastLoaded))
        .collect();
    let waiting = queue(&shared, 0, Target::LeastLoaded);
    shared.workers[0].heartbeat.begin(0, 1, "blocker");
    clock.advance(20_000);
    monitor::scan(&shared, clock.now_micros());
    assert_eq!(shared.workers[0].load(), 1);
    assert_eq!(shared.workers[0].stats().moved_out, 0);
    drop((full, waiting));
}

#[test]
fn a_key_keeps_its_worker_and_falls_back_to_one_peer_while_it_is_stuck() {
    let shared = bare(4, Arc::new(ManualClock::default()));
    let key = key_on(&shared, 0);
    assert_eq!(shared.route(Target::Key(key)).unwrap().id.index, 0);
    let mut used = [false; 4];
    for k in 0..400u64 {
        used[shared
            .key_worker(k.wrapping_mul(0x9e37_79b9_7f4a_7c15))
            .id
            .index] = true;
    }
    assert!(used.iter().all(|u| *u), "keys spread over every worker");
    shared.workers[0].stuck.store(true, Ordering::SeqCst);
    let fallback = shared.route(Target::Key(key)).unwrap().id.index;
    assert_ne!(fallback, 0);
    for _ in 0..20 {
        assert_eq!(
            shared.route(Target::Key(key)).unwrap().id.index,
            fallback,
            "the same fallback each time"
        );
    }
}

#[test]
fn unknown_workers_and_nodes_are_errors() {
    let shared = bare(2, Arc::new(ManualClock::default()));
    assert_eq!(
        shared.route(Target::Worker(2)).err(),
        Some(Error::NoSuchWorker {
            index: 2,
            workers: 2
        })
    );
    assert_eq!(
        shared.route(Target::Node(3)).err(),
        Some(Error::NoSuchNode(3))
    );
    assert!(shared.route(Target::Node(0)).is_ok());
}

#[test]
fn the_least_loaded_of_two_choices_takes_the_lighter_worker() {
    let shared = bare(2, Arc::new(ManualClock::default()));
    let held: Vec<_> = (0..3)
        .map(|_| queue(&shared, 0, Target::LeastLoaded))
        .collect();
    let picks = (0..200)
        .filter(|_| shared.route(Target::LeastLoaded).unwrap().id.index == 1)
        .count();
    assert!(
        picks > 120,
        "every draw that sees both picks the lighter: {picks}"
    );
    drop(held);
}

#[test]
fn work_runs_on_the_worker_its_target_names() {
    let pool = Pool::start(small(3)).unwrap();
    for index in 0..3 {
        let ran_on = pool.submit_and_wait(Target::Worker(index), || async {
            current_worker().map(|w| w.index)
        });
        assert_eq!(ran_on, Ok(Some(index)));
    }
    let first = pool.submit_and_wait(Target::Key(77), || async {
        current_worker().map(|w| w.index)
    });
    let second = pool.submit_and_wait(Target::Key(77), || async {
        current_worker().map(|w| w.index)
    });
    assert_eq!(first, second);
    assert!(pool
        .submit_and_wait(Target::LeastLoaded, || async { 1 })
        .is_ok());
    assert!(pool
        .submit_and_wait(Target::Node(pool.nodes()[0]), || async { 1 })
        .is_ok());
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_worker_waiting_on_itself_gets_an_error_not_a_deadlock() {
    let pool = Pool::start(small(1)).unwrap();
    let inner = pool.clone();
    let result = pool.submit_and_wait(Target::Worker(0), move || async move {
        (
            block_on(async {}),
            inner.submit_and_wait(Target::Worker(0), || async {}),
        )
    });
    assert_eq!(result, Ok((Err(Error::OnWorker), Err(Error::OnWorker))));
    assert_eq!(
        pool.submit_and_wait(Target::Worker(0), || async { Pool::start(small(1)).err() }),
        Ok(Some(Error::OnWorker))
    );
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_panic_fails_its_own_task_and_the_worker_goes_on() {
    let pool = Pool::start(small(1)).unwrap();
    let failed = pool.submit_and_wait(Target::Worker(0), || async { panic!("boom") });
    assert!(
        matches!(failed, Err(Error::Panicked { ref message, .. }) if message == "boom"),
        "{failed:?}"
    );
    assert_eq!(
        pool.submit_and_wait(Target::Worker(0), || async { 2 }),
        Ok(2)
    );
    assert_eq!(pool.stats().workers[0].panics, 1);
    pool.shutdown_blocking().unwrap();
}

#[test]
fn admission_waits_for_room_and_never_overfills_a_worker() {
    let pool = Pool::start(Config {
        inbox_items: 2,
        ..small(1)
    })
    .unwrap();
    let gate = Arc::new(Event::new());
    let held: Vec<_> = (0..2)
        .map(|_| {
            let gate = gate.clone();
            block_on(pool.submit(Target::Worker(0), move || async move { gate.wait().await }))
                .unwrap()
                .unwrap()
        })
        .collect();
    let mut third = Box::pin(pool.submit(Target::Worker(0), || async { 3 }));
    assert!(poll_once(third.as_mut()).is_pending(), "the worker is full");
    assert_eq!(pool.stats().workers[0].load, 2);
    gate.set();
    held.into_iter().for_each(|done| done.wait().unwrap());
    let third = block_on(third).unwrap().unwrap();
    assert_eq!(third.wait(), Ok(3));
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_submissions_bytes_count_against_the_byte_bound() {
    let pool = Pool::start(Config {
        inbox_bytes: 100,
        ..small(1)
    })
    .unwrap();
    let gate = Arc::new(Event::new());
    let big = Submit {
        name: Some("big"),
        bytes: Some(60),
    };
    let first = {
        let gate = gate.clone();
        block_on(pool.submit_with(
            Target::Worker(0),
            big,
            move || async move { gate.wait().await },
        ))
        .unwrap()
        .unwrap()
    };
    let mut second = Box::pin(pool.submit_with(Target::Worker(0), big, || async {}));
    assert!(
        poll_once(second.as_mut()).is_pending(),
        "60 + 60 bytes exceed 100"
    );
    gate.set();
    first.wait().unwrap();
    block_on(second).unwrap().unwrap().wait().unwrap();
    let huge = Submit {
        name: None,
        bytes: Some(10_000),
    };
    let alone = block_on(pool.submit_with(Target::Worker(0), huge, || async { 2 }))
        .unwrap()
        .unwrap();
    assert_eq!(
        alone.wait(),
        Ok(2),
        "a submission larger than the bound still runs alone"
    );
    pool.shutdown_blocking().unwrap();
}

#[test]
fn blocking_code_runs_on_the_blocking_pool_and_is_counted() {
    let pool = Pool::start(small(1)).unwrap();
    let inner = pool.clone();
    let name = pool
        .submit_and_wait(Target::Worker(0), move || async move {
            let done = inner
                .spawn_blocking(|| std::thread::current().name().map(str::to_owned))
                .await?;
            done.await
        })
        .unwrap()
        .unwrap();
    assert!(name.unwrap().contains("blocking"));
    let panicked = block_on(pool.spawn_blocking(|| panic!("sync boom")))
        .unwrap()
        .unwrap()
        .wait();
    assert!(matches!(panicked, Err::<(), _>(Error::Panicked { .. })));
    assert_eq!(pool.stats().blocking_spawned, 2);
    pool.shutdown_blocking().unwrap();
}

#[test]
fn shutdown_finishes_admitted_work_then_refuses_more() {
    let pool = Pool::start(small(2)).unwrap();
    let done = block_on(pool.submit(Target::Worker(1), || async {
        crate::sleep(Duration::from_millis(5)).unwrap().await;
        9
    }))
    .unwrap()
    .unwrap();
    pool.shutdown_blocking().unwrap();
    assert_eq!(done.wait(), Ok(9));
    assert_eq!(
        block_on(pool.submit(Target::Worker(0), || async {}))
            .unwrap()
            .err(),
        Some(Error::ShutDown)
    );
}

#[test]
fn shutdown_past_its_deadline_abandons_what_is_left() {
    let pool = Pool::start(Config {
        shutdown_deadline: Duration::from_millis(20),
        ..small(1)
    })
    .unwrap();
    let never = block_on(pool.submit(Target::Worker(0), std::future::pending::<()>))
        .unwrap()
        .unwrap();
    assert_eq!(pool.shutdown_blocking(), Err(Error::TimedOut));
    assert_eq!(never.wait(), Err(Error::ShutDown));
}

#[test]
fn a_one_worker_pool_serves_every_target_and_measures_its_own_polls() {
    let pool = Pool::start(Config {
        stuck_after: Duration::from_millis(1),
        ..small(1)
    })
    .unwrap();
    assert_eq!(pool.workers(), 1);
    assert_eq!(pool.submit_and_wait(Target::Key(5), || async { 5 }), Ok(5));
    assert_eq!(
        pool.submit_and_wait(Target::LeastLoaded, || async { 6 }),
        Ok(6)
    );
    let blocked = pool.submit_and_wait(Target::Worker(0), || async {
        std::thread::sleep(Duration::from_millis(20));
    });
    assert_eq!(blocked, Ok(()));
    assert!(pool.stats().workers[0].long_polls >= 1);
    pool.shutdown_blocking().unwrap();
}

struct Count(AtomicUsize);

impl Wake for Count {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn a_driven_pool_runs_on_the_callers_thread_when_woken() {
    let wakes = Arc::new(Count(AtomicUsize::new(0)));
    let (pool, mut driver) = Pool::driven(small(4), Waker::from(wakes.clone())).unwrap();
    assert_eq!(pool.workers(), 1);
    assert_eq!(driver.turn(), 0);
    let submitter = {
        let pool = pool.clone();
        std::thread::spawn(move || block_on(pool.submit(Target::LeastLoaded, || async { 6 * 7 })))
    };
    let mut done = submitter.join().unwrap().unwrap().unwrap();
    assert_eq!(
        wakes.0.load(Ordering::SeqCst),
        1,
        "the push woke the caller once"
    );
    while driver.turn() > 0 {}
    assert_eq!(poll_once(Pin::new(&mut done)), Poll::Ready(Ok(42)));
    let local = crate::spawn_local(async { 1 }).unwrap();
    while driver.turn() > 0 {}
    assert!(
        local.is_finished(),
        "spawn_local works on the driving thread"
    );
    assert_eq!(
        Pool::driven(small(1), Waker::noop().clone()).err(),
        Some(Error::OnWorker),
        "one thread drives one worker"
    );
    pool.shutdown();
    while driver.turn() > 0 {}
    assert!(driver.finished());
}

#[test]
fn a_thread_driving_its_pool_blocks_on_a_future() {
    std::thread::spawn(|| {
        let (pool, mut driver) = Pool::driven_here(small(1)).unwrap();
        let other = pool.clone();
        let out = driver.block_on(async move {
            crate::sleep(Duration::from_millis(2)).unwrap().await;
            let blocking = other.spawn_blocking(|| 20).await.unwrap().await.unwrap();
            blocking + 1
        });
        assert_eq!(out, Ok(21));
    })
    .join()
    .unwrap();
    let (_pool, mut woken) = Pool::driven(small(1), Waker::noop().clone()).unwrap();
    assert!(
        matches!(woken.block_on(async {}), Err(Error::Config(_))),
        "only a parking driver blocks"
    );
}

#[test]
fn the_hook_is_polled_between_tasks_on_its_worker() {
    struct Counting(Arc<AtomicUsize>);
    impl Hook for Counting {
        fn poll(&mut self, _budget: usize) -> HookProgress {
            self.0.fetch_add(1, Ordering::SeqCst);
            HookProgress::default()
        }
        fn idle(&mut self, _waker: &Waker) {}
    }
    let polls = Arc::new(AtomicUsize::new(0));
    let counted = polls.clone();
    let config = Config {
        hook: Some(Arc::new(move |_| Box::new(Counting(counted.clone())))),
        ..small(1)
    };
    let pool = Pool::start(config).unwrap();
    pool.submit_and_wait(Target::Worker(0), || async {})
        .unwrap();
    assert!(polls.load(Ordering::SeqCst) > 0);
    pool.shutdown_blocking().unwrap();

    let panicking = Config {
        hook: Some(Arc::new(|_| panic!("no queue"))),
        ..small(1)
    };
    assert!(matches!(Pool::start(panicking), Err(Error::Spawn(_))));
}

#[test]
fn a_regolith_io_queue_serves_as_the_hook() {
    let db = Arc::new(regolith::Db::open("hook-pool", regolith::Options::memory()).unwrap());
    let queues = db.clone();
    let config = Config {
        hook: Some(Arc::new(move |_| Box::new(queues.io_queue()))),
        ..small(1)
    };
    let pool = Pool::start(config).unwrap();
    let writer = db.clone();
    let read = pool.submit_and_wait(Target::Worker(0), move || async move {
        writer.put(b"k", b"v").unwrap();
        writer.get(b"k").unwrap().map(|v| v.to_vec())
    });
    assert_eq!(read, Ok(Some(b"v".to_vec())));
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_driven_pool_whose_hook_factory_panics_is_an_error() {
    std::thread::spawn(|| {
        let config = Config {
            hook: Some(Arc::new(|_| panic!("no queue"))),
            ..small(1)
        };
        assert!(matches!(
            Pool::driven(config, Waker::noop().clone()),
            Err(Error::Spawn(_))
        ));
        assert!(!crate::on_worker(), "nothing was left half made");
    })
    .join()
    .unwrap();
}

#[test]
fn a_submission_is_send_though_the_task_it_builds_is_not() {
    fn send<T>(_: &T)
    where
        T: Send,
    {
    }
    let pool = Pool::start(small(1)).unwrap();
    let submission = pool.submit(Target::LeastLoaded, || async {
        let not_send = std::rc::Rc::new(1);
        crate::yield_now().await;
        *not_send
    });
    send(&submission);
    let done = block_on(submission).unwrap().unwrap();
    assert_eq!(done.wait(), Ok(1));
    pool.shutdown_blocking().unwrap();
}

#[test]
fn the_pool_and_its_handles_cross_threads() {
    fn send_sync<T>()
    where
        T: Send + Sync,
    {
    }
    send_sync::<Pool>();
    send_sync::<Completion<String>>();
    send_sync::<PlacedEnv>();
}

#[cfg(target_os = "linux")]
#[test]
fn every_worker_runs_pinned_where_the_plan_put_it() {
    let allowed = crate::os::current_affinity().unwrap();
    let workers = allowed.len().min(3);
    let pool = Pool::start(Config {
        placement: Placement::Spread,
        ..small(workers)
    })
    .unwrap();
    let mut seen = CpuSet::new();
    for (index, worker) in pool.report().workers.iter().enumerate() {
        let pinning = &worker.placement.pinning;
        assert_eq!(pinning.fell_back, None, "{pinning:?}");
        let planned = CpuSet::parse(pinning.granted.trim_start_matches("CPUs ")).unwrap();
        assert_eq!(planned.len(), 1);
        assert!(allowed.contains(planned.first().unwrap()));
        assert!(
            seen.intersection(&planned).is_empty(),
            "no two workers share a CPU"
        );
        seen.union_with(&planned);
        let (mask, cpu) = pool
            .submit_and_wait(Target::Worker(index), || async {
                (
                    crate::os::current_affinity(),
                    crate::os::system().current_cpu(),
                )
            })
            .unwrap();
        assert_eq!(mask.as_ref(), Some(&planned));
        assert!(planned.contains(cpu.unwrap()));
    }
    assert_eq!(
        pool.stats().workers.iter().map(|w| w.repins).sum::<u64>(),
        0
    );
    pool.shutdown_blocking().unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn compaction_and_edge_threads_run_on_the_spare_cpus() {
    let allowed = crate::os::current_affinity().unwrap();
    if allowed.len() < 2 {
        return;
    }
    let pool = Pool::start(Config {
        placement: Placement::Spread,
        ..small(1)
    })
    .unwrap();
    let env = pool.env(regolith::env::std_env());
    let (sender, done) = crate::completion::channel();
    let thread = env
        .spawn(
            "regolith-compaction-0",
            Box::new(move || sender.send(crate::os::current_affinity())),
        )
        .unwrap();
    thread.join();
    let mask = done.wait().unwrap().unwrap();
    let pinning = &pool.report().workers[0].placement.pinning;
    let worker = CpuSet::parse(pinning.granted.trim_start_matches("CPUs ")).unwrap();
    assert!(
        mask.intersection(&worker).is_empty(),
        "compaction stays off the worker's CPU: {mask} vs {worker}"
    );
    assert!(
        env.now_micros().is_some(),
        "other calls reach the wrapped env"
    );
    let edge = std::thread::spawn({
        let pool = pool.clone();
        move || pool.place_current_thread(ThreadClass::Edge)
    })
    .join()
    .unwrap();
    assert_eq!(edge.pinning.fell_back, None, "{edge:?}");
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_submission_that_may_not_wait_is_refused_while_its_worker_is_full() {
    let pool = Pool::start(Config {
        inbox_items: 1,
        ..small(1)
    })
    .unwrap();
    let gate = Arc::new(Event::new());
    let held = gate.clone();
    let first = pool
        .try_submit_with(Target::Worker(0), Submit::default(), move || async move {
            held.wait().await;
            1
        })
        .unwrap();
    assert_eq!(
        pool.try_submit_with(Target::Worker(0), Submit::default(), || async { 2 })
            .err(),
        Some(Error::Full),
        "the one admission is held by the running job"
    );
    gate.set();
    assert_eq!(first.wait(), Ok(1));
    assert_eq!(
        pool.submit_and_wait(Target::Worker(0), || async { 3 }),
        Ok(3),
        "a refused submission leaves nothing behind"
    );
    pool.shutdown_blocking().unwrap();
}

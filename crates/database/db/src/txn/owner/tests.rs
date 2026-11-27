//! The owner mechanism on a real pool, with a transaction that records what
//! happens to it: routing by the ID's owner, one request at a time in arrival
//! order, the idle sweep against a request that arrived first, transactions
//! on different workers running side by side, and every way a transaction
//! ends without a commit dropping it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use regolith::sync::Event;
use workers::{block_on, Config, Placement, Pool, Workers};

use super::{Outcome, OutcomeRecord, OutcomeStore, Owners, TxnId, Unavailable, OUTCOME_CAP};

/// A transaction that names its worker and records its drop.
struct Probe {
    opened_on: usize,
    log: Vec<String>,
    dropped: Arc<AtomicUsize>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

fn pool(workers: usize) -> Pool {
    Pool::start(Config {
        workers: Workers::Count(workers),
        placement: Placement::Unpinned,
        idle_spin: Duration::from_micros(10),
        blocking_threads: 1,
        ..Config::default()
    })
    .unwrap()
}

type Probes = Owners<Probe, &'static str>;

fn begin(owners: &Probes, owner: &'static str, dropped: &Arc<AtomicUsize>) -> TxnId {
    let dropped = dropped.clone();
    block_on(owners.begin(owner, move |_| async move {
        Ok::<_, ()>(Probe {
            opened_on: workers::current_worker().unwrap().index,
            log: Vec::new(),
            dropped,
        })
    }))
    .unwrap()
    .unwrap()
    .unwrap()
}

#[test]
fn a_request_runs_on_the_worker_its_id_names() {
    let owners = Probes::new(pool(3));
    let dropped = Arc::new(AtomicUsize::new(0));
    for _ in 0..12 {
        let id = begin(&owners, "alice", &dropped);
        let (ran_on, opened_on) = block_on(owners.run(id, &"alice", |probe| {
            Box::pin(async move { (workers::current_worker().unwrap().index, probe.opened_on) })
        }))
        .unwrap()
        .unwrap();
        assert_eq!(ran_on, id.owner(3));
        assert_eq!(opened_on, id.owner(3), "the worker that opened it owns it");
    }
    assert_eq!(owners.len(), 12);
}

#[test]
fn requests_for_one_transaction_run_one_at_a_time_in_arrival_order() {
    let owners = Probes::new(pool(2));
    let dropped = Arc::new(AtomicUsize::new(0));
    let id = begin(&owners, "alice", &dropped);
    let requests = (0..16).map(|i| {
        owners.run(id, &"alice", move |probe| {
            Box::pin(async move {
                probe.log.push(format!("start {i}"));
                workers::yield_now().await;
                probe.log.push(format!("end {i}"));
            })
        })
    });
    for done in block_on(join_all(requests)).unwrap() {
        done.unwrap();
    }
    let log = block_on(owners.run(id, &"alice", |probe| {
        Box::pin(async move { std::mem::take(&mut probe.log) })
    }))
    .unwrap()
    .unwrap();
    let expected: Vec<String> = (0..16)
        .flat_map(|i| [format!("start {i}"), format!("end {i}")])
        .collect();
    assert_eq!(log, expected);
}

#[test]
fn a_caller_that_does_not_own_the_transaction_finds_nothing_and_changes_nothing() {
    let owners = Probes::new(pool(1));
    let dropped = Arc::new(AtomicUsize::new(0));
    let id = begin(&owners, "alice", &dropped);
    let foreign = block_on(owners.run(id, &"bob", |probe| {
        Box::pin(async move { probe.log.push("bob".into()) })
    }))
    .unwrap();
    assert_eq!(foreign, Err(Unavailable::NotFound));
    let finished =
        block_on(owners.finish(id, &"bob", |probe| Box::pin(async move { drop(probe) }))).unwrap();
    assert_eq!(finished, Err(Unavailable::NotFound));
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    let log = block_on(owners.finish(id, &"alice", |mut probe| {
        Box::pin(async move { std::mem::take(&mut probe.log) })
    }))
    .unwrap()
    .unwrap();
    assert!(log.is_empty(), "the refused request never ran: {log:?}");
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(owners.len(), 0);
    let after = block_on(owners.run(id, &"alice", |_| Box::pin(async {}))).unwrap();
    assert_eq!(
        after,
        Err(Unavailable::NotFound),
        "a finished transaction is gone"
    );
}

#[test]
fn the_sweep_never_takes_a_transaction_a_request_reached_first() {
    let owners = Arc::new(Probes::new(pool(1)));
    let dropped = Arc::new(AtomicUsize::new(0));
    let id = begin(&owners, "alice", &dropped);
    let max_idle = Duration::from_millis(50);
    // Idle past the limit before anything else happens.
    std::thread::sleep(max_idle + Duration::from_millis(20));

    let gate = Arc::new(Event::new());
    let entered = Arc::new(Event::new());
    let request = {
        let (owners, gate, entered) = (owners.clone(), gate.clone(), entered.clone());
        std::thread::spawn(move || {
            block_on(owners.run(id, &"alice", move |probe| {
                Box::pin(async move {
                    entered.set();
                    gate.wait().await;
                    probe.log.push("kept".into());
                })
            }))
        })
    };
    block_on(entered.wait()).unwrap();
    // The sweep's message queues behind the request that is running.
    let sweep = {
        let owners = owners.clone();
        std::thread::spawn(move || block_on(owners.expire_idle(max_idle)))
    };
    gate.set();
    assert_eq!(request.join().unwrap().unwrap(), Ok(()));
    assert_eq!(sweep.join().unwrap().unwrap(), Ok(0));
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    assert_eq!(owners.len(), 1);

    std::thread::sleep(max_idle + Duration::from_millis(20));
    assert_eq!(block_on(owners.expire_idle(max_idle)).unwrap(), Ok(1));
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    let after = block_on(owners.run(id, &"alice", |_| Box::pin(async {}))).unwrap();
    assert_eq!(after, Err(Unavailable::NotFound));
}

#[test]
fn transactions_on_different_workers_run_side_by_side() {
    let owners = Arc::new(Probes::new(pool(2)));
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut by_worker: [Option<TxnId>; 2] = [None, None];
    for _ in 0..256 {
        let id = begin(&owners, "alice", &dropped);
        by_worker[id.owner(2)].get_or_insert(id);
        if by_worker.iter().all(Option::is_some) {
            break;
        }
    }
    let [Some(first), Some(second)] = by_worker else {
        panic!("least-loaded placement never used both workers");
    };
    let gate = Arc::new(Event::new());
    let entered = Arc::new(Event::new());
    let blocked = {
        let (owners, gate, entered) = (owners.clone(), gate.clone(), entered.clone());
        std::thread::spawn(move || {
            block_on(owners.run(first, &"alice", move |_| {
                Box::pin(async move {
                    entered.set();
                    gate.wait().await;
                })
            }))
        })
    };
    block_on(entered.wait()).unwrap();
    let other = block_on(owners.run(second, &"alice", |probe| {
        Box::pin(async move { probe.opened_on })
    }))
    .unwrap();
    assert_eq!(
        other,
        Ok(second.owner(2)),
        "ran while the first worker's request waited"
    );
    gate.set();
    assert_eq!(blocked.join().unwrap().unwrap(), Ok(()));
}

#[test]
fn abandoning_or_dropping_the_registry_ends_the_transaction() {
    let pool = pool(1);
    let owners = Probes::new(pool.clone());
    let dropped = Arc::new(AtomicUsize::new(0));
    let abandoned = begin(&owners, "alice", &dropped);
    owners.abandon(abandoned);
    let after = block_on(owners.run(abandoned, &"alice", |_| Box::pin(async {}))).unwrap();
    assert_eq!(after, Err(Unavailable::NotFound), "the abandon came first");
    assert_eq!(dropped.load(Ordering::SeqCst), 1);

    let open: Vec<_> = (0..3).map(|_| begin(&owners, "alice", &dropped)).collect();
    assert_eq!(owners.len(), open.len());
    drop(owners);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while dropped.load(Ordering::SeqCst) < 4 {
        assert!(
            std::time::Instant::now() < deadline,
            "transactions outlived their registry"
        );
        std::thread::yield_now();
    }
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_commit_whose_waiter_leaves_before_it_begins_still_lands() {
    let owners = Arc::new(Probes::new(pool(1)));
    let dropped = Arc::new(AtomicUsize::new(0));
    let committed = Arc::new(AtomicUsize::new(0));
    let request_entered = Arc::new(Event::new());
    let request_released = Arc::new(Event::new());
    let landed = Arc::new(Event::new());
    let id = begin(&owners, "alice", &dropped);
    let request = {
        let (owners, entered, released) = (
            owners.clone(),
            request_entered.clone(),
            request_released.clone(),
        );
        std::thread::spawn(move || {
            block_on(owners.run(id, &"alice", move |_| {
                Box::pin(async move {
                    entered.set();
                    released.wait().await;
                })
            }))
        })
    };
    block_on(request_entered.wait()).unwrap();
    let waiter = std::thread::spawn({
        let (owners, committed, landed) = (owners.clone(), committed.clone(), landed.clone());
        move || {
            block_on(async {
                workers::timeout(Duration::from_millis(20), async {
                    owners
                        .finish_detached(id, &"alice", |mut txn| {
                            Box::pin(async move {
                                txn.log.push("committed".into());
                                committed.fetch_add(1, Ordering::SeqCst);
                                landed.set();
                                Ok::<(), std::convert::Infallible>(())
                            })
                        })
                        .await
                })
                .await
                .is_err()
            })
        }
    });
    assert!(
        waiter.join().unwrap().unwrap(),
        "the commit wait did not time out while it queued behind a request"
    );
    assert_eq!(
        committed.load(Ordering::SeqCst),
        0,
        "the commit began before its waiter gave up"
    );
    request_released.set();
    assert_eq!(request.join().unwrap().unwrap(), Ok(()));
    block_on(workers::timeout(Duration::from_secs(5), landed.wait()))
        .unwrap()
        .expect("the queued commit never ran after its waiter left");
    let after = block_on(owners.run(id, &"alice", |probe| {
        Box::pin(async move { probe.log.clone() })
    }))
    .unwrap();
    assert_eq!(after, Err(Unavailable::NotFound), "the commit ended it");
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

#[test]
fn a_standalone_read_called_on_a_worker_runs_on_that_worker() {
    let pool = pool(4);
    for index in 0..4 {
        let on_pool = pool.clone();
        let ran = block_on(async {
            pool.submit(workers::Target::Worker(index), move || async move {
                let owners = Probes::new(on_pool);
                owners
                    .run_anywhere("read", || async {
                        workers::current_worker().unwrap().index
                    })
                    .await
            })
            .await
            .unwrap()
            .await
        })
        .unwrap()
        .unwrap();
        assert_eq!(
            ran,
            Ok(index),
            "the read stays on the worker it was called from"
        );
    }
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_standalone_read_called_off_the_workers_runs_on_one() {
    let owners = Probes::new(pool(4));
    let ran = block_on(owners.run_anywhere("read", || async {
        workers::current_worker().map(|worker| worker.index)
    }))
    .unwrap()
    .unwrap();
    assert!(ran.is_some(), "the read runs on a worker of the pool");
}

#[test]
fn an_outcome_is_open_then_final_only_for_its_owner() {
    let owners = Probes::new(pool(1));
    let dropped = Arc::new(AtomicUsize::new(0));
    let id = begin(&owners, "alice", &dropped);

    assert_eq!(owners.outcome(id, &"alice"), Some(Outcome::Open));
    assert_eq!(owners.outcome(id, &"bob"), None);

    let status = owners.outcome_status(id, &"alice").unwrap();
    Outcome::Committing.store(&status);
    assert_eq!(owners.outcome(id, &"alice"), Some(Outcome::Committing));
    Outcome::Committed.store(&status);

    block_on(owners.finish(id, &"alice", |probe| Box::pin(async move { drop(probe) })))
        .unwrap()
        .unwrap();
    assert_eq!(owners.outcome(id, &"alice"), Some(Outcome::Committed));
    assert_eq!(owners.outcome(id, &"bob"), None);
}

#[test]
fn an_id_from_another_start_names_neither_the_live_transaction_nor_its_outcome() {
    let owners = Probes::new(pool(1));
    let dropped = Arc::new(AtomicUsize::new(0));
    let id = begin(&owners, "alice", &dropped);
    let (start, route) = id.key();
    let foreign = TxnId::from_key((start ^ (1u64 << 32), route));
    assert_eq!(foreign.owner(1), id.owner(1));

    assert_eq!(owners.outcome(id, &"alice"), Some(Outcome::Open));
    assert_eq!(owners.outcome(foreign, &"alice"), None);
    assert!(owners.outcome_status(foreign, &"alice").is_none());
    assert_eq!(
        block_on(owners.run(foreign, &"alice", |_| Box::pin(async {}))).unwrap(),
        Err(Unavailable::NotFound)
    );

    let status = owners.outcome_status(id, &"alice").unwrap();
    Outcome::Committed.store(&status);
    block_on(owners.finish(id, &"alice", |probe| Box::pin(async move { drop(probe) })))
        .unwrap()
        .unwrap();
    assert_eq!(owners.outcome(id, &"alice"), Some(Outcome::Committed));
    assert_eq!(owners.outcome(foreign, &"alice"), None);
}

#[test]
fn outcome_retention_keeps_final_results_and_not_open_work() {
    let store: OutcomeStore<&'static str> =
        OutcomeStore::new(Duration::from_millis(20), OUTCOME_CAP);
    let now = std::time::Instant::now();
    let id = TxnId::new(1, 0, 1, 17).unwrap();
    let cell = Arc::new(std::sync::atomic::AtomicU8::new(Outcome::Open as u8));
    store.insert(
        id,
        OutcomeRecord {
            status: Arc::clone(&cell),
            owner: "alice",
            finished: now,
        },
    );

    std::thread::sleep(Duration::from_millis(30));
    store.expire(std::time::Instant::now());
    assert_eq!(store.status(id), Some(Outcome::Open));

    store.mark(id, Outcome::Aborted, std::time::Instant::now());
    assert_eq!(store.status(id), Some(Outcome::Aborted));
    std::thread::sleep(Duration::from_millis(30));
    store.expire(std::time::Instant::now());
    assert_eq!(store.status(id), None);
}

#[test]
fn outcome_records_stay_bounded_when_retention_is_disabled() {
    let store: OutcomeStore<&'static str> = OutcomeStore::new(Duration::ZERO, 3);
    let now = std::time::Instant::now();
    let later = now + Duration::from_millis(1);
    let latest = now + Duration::from_millis(2);
    for index in 0..4 {
        let finished = match index {
            0 => now,
            1 => later,
            _ => latest,
        };
        let id = TxnId::new(index, 0, 1, 1).unwrap();
        store.insert(
            id,
            OutcomeRecord {
                status: Arc::new(std::sync::atomic::AtomicU8::new(Outcome::Aborted as u8)),
                owner: "alice",
                finished,
            },
        );
    }

    assert_eq!(store.records.len(), 3);
    assert_eq!(store.evictions(), 1);
    assert_eq!(store.status(TxnId::new(0, 0, 1, 1).unwrap()), None);
    assert_eq!(
        store.status(TxnId::new(1, 0, 1, 1).unwrap()),
        Some(Outcome::Aborted)
    );
    assert_eq!(
        store.status(TxnId::new(2, 0, 1, 1).unwrap()),
        Some(Outcome::Aborted)
    );
    assert_eq!(
        store.status(TxnId::new(3, 0, 1, 1).unwrap()),
        Some(Outcome::Aborted)
    );
}

#[test]
fn a_full_outcome_store_evicts_a_final_result_before_a_live_record() {
    let store: OutcomeStore<&'static str> = OutcomeStore::new(Duration::ZERO, 3);
    let now = std::time::Instant::now();
    let record = |status: Outcome, age: u64| OutcomeRecord {
        status: Arc::new(std::sync::atomic::AtomicU8::new(status as u8)),
        owner: "alice",
        finished: now + Duration::from_millis(age),
    };
    let live = TxnId::new(0, 0, 1, 1).unwrap();
    let older = TxnId::new(1, 0, 1, 1).unwrap();
    let newer = TxnId::new(2, 0, 1, 1).unwrap();
    store.insert(live, record(Outcome::Open, 0));
    store.insert(older, record(Outcome::Committed, 1));
    store.insert(newer, record(Outcome::Aborted, 2));

    store.insert(TxnId::new(3, 0, 1, 1).unwrap(), record(Outcome::Open, 3));

    assert_eq!(store.evictions(), 1);
    assert_eq!(store.status(live), Some(Outcome::Open));
    assert_eq!(store.status(older), None);
    assert_eq!(store.status(newer), Some(Outcome::Aborted));
}

#[test]
fn a_full_outcome_store_frees_an_eighth_of_its_cap_in_one_pass() {
    let store: OutcomeStore<&'static str> = OutcomeStore::new(Duration::ZERO, 16);
    let now = std::time::Instant::now();
    let record = |status: Outcome, age: u64| OutcomeRecord {
        status: Arc::new(std::sync::atomic::AtomicU8::new(status as u8)),
        owner: "alice",
        finished: now + Duration::from_millis(age),
    };
    for index in 0..16 {
        let status = if index < 4 {
            Outcome::Open
        } else {
            Outcome::Committed
        };
        store.insert(TxnId::new(index, 0, 1, 1).unwrap(), record(status, index));
    }

    store.insert(TxnId::new(16, 0, 1, 1).unwrap(), record(Outcome::Open, 16));
    assert_eq!(store.evictions(), 2);
    assert_eq!(store.records.len(), 15);
    for index in 0..4 {
        assert_eq!(
            store.status(TxnId::new(index, 0, 1, 1).unwrap()),
            Some(Outcome::Open)
        );
    }
    assert_eq!(store.status(TxnId::new(4, 0, 1, 1).unwrap()), None);
    assert_eq!(store.status(TxnId::new(5, 0, 1, 1).unwrap()), None);
    assert_eq!(
        store.status(TxnId::new(6, 0, 1, 1).unwrap()),
        Some(Outcome::Committed)
    );

    store.insert(TxnId::new(17, 0, 1, 1).unwrap(), record(Outcome::Open, 17));
    assert_eq!(store.evictions(), 2);
}

#[test]
fn a_full_outcome_store_never_evicts_a_live_record() {
    let store: OutcomeStore<&'static str> = OutcomeStore::new(Duration::ZERO, 4);
    let now = std::time::Instant::now();
    let cells: Vec<_> = (0..6)
        .map(|index| {
            let cell = Arc::new(std::sync::atomic::AtomicU8::new(Outcome::Open as u8));
            store.insert(
                TxnId::new(index, 0, 1, 1).unwrap(),
                OutcomeRecord {
                    status: Arc::clone(&cell),
                    owner: "alice",
                    finished: now + Duration::from_millis(index),
                },
            );
            cell
        })
        .collect();
    assert_eq!(store.evictions(), 0);
    assert_eq!(store.records.len(), 6);

    for cell in &cells[..3] {
        Outcome::Committed.store(cell);
    }
    store.insert(
        TxnId::new(6, 0, 1, 1).unwrap(),
        OutcomeRecord {
            status: Arc::new(std::sync::atomic::AtomicU8::new(Outcome::Open as u8)),
            owner: "alice",
            finished: now + Duration::from_millis(6),
        },
    );
    assert_eq!(store.evictions(), 3);
    for index in 0..3 {
        assert_eq!(store.status(TxnId::new(index, 0, 1, 1).unwrap()), None);
    }
    for index in 3..7 {
        assert_eq!(
            store.status(TxnId::new(index, 0, 1, 1).unwrap()),
            Some(Outcome::Open)
        );
    }
}

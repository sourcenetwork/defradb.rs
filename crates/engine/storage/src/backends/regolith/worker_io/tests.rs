//! A store on a pool: each worker reads on its own queue and commits through
//! tickets its own queue completes; off the pool nothing changes.

use std::time::Duration;

use crate::corekv::{OnCommit, Reader, Store, Writer};
use crate::{with_worker_io, RegolithStore, RegolithStoreOptions};

/// Every wait in these tests carries a deadline, so a regression fails
/// instead of hanging the suite.
const DEADLINE: Duration = Duration::from_secs(60);

fn pool(workers: usize) -> workers::Pool {
    workers::Pool::start(with_worker_io(workers::Config::unplaced(workers))).unwrap()
}

/// Writes `count` values of 16 KiB under `cold/`, flushes them to a table and
/// reopens the store on `pool`, so every read of them misses the cache.
fn cold_store(dir: &std::path::Path, count: u8, pool: &workers::Pool) -> RegolithStore {
    let store = RegolithStore::open(dir).unwrap();
    workers::block_on(async {
        let mut txn = store.new_txn(false).await.unwrap();
        for n in 0..count {
            txn.set(format!("cold/{n:02}").as_bytes(), &vec![n; 16384])
                .await
                .unwrap();
        }
        txn.commit().await.unwrap();
        store.engine().db().flush().unwrap();
        store.close().await.unwrap();
    })
    .unwrap();
    drop(store);
    RegolithStore::open_with_options(dir, RegolithStoreOptions::default().placed_on(pool)).unwrap()
}

/// A worker opens one queue per database, on its own thread; a plain thread
/// has none.
#[test]
fn a_worker_has_one_queue_per_database_and_a_plain_thread_none() {
    let store = RegolithStore::in_memory().unwrap();
    let db = store.engine_arc().clone();
    assert_eq!(super::queue_for(&db), None);

    let pool = pool(1);
    let on_worker = db.clone();
    let (first, again) = pool
        .submit_and_wait(workers::Target::Worker(0), move || async move {
            (super::queue_for(&on_worker), super::queue_for(&on_worker))
        })
        .unwrap();
    assert!(first.is_some());
    assert_eq!(first, again, "the second lookup reuses the queue");
    pool.shutdown_blocking().unwrap();
}

/// Reads that miss the cache on a worker suspend on the worker's queue and
/// complete: point reads, a read-only scan and a writing transaction's scan.
/// Before the worker polled its queue between tasks, the first miss never
/// completed.
#[test]
fn cold_reads_on_a_worker_complete_through_its_queue() {
    let dir = tempfile::tempdir().unwrap();
    let pool = pool(2);
    let store = cold_store(dir.path(), 24, &pool);
    let on_worker = store.clone();
    let done = workers::block_on(
        pool.submit(workers::Target::LeastLoaded, move || async move {
            let reader = on_worker.new_txn(true).await.unwrap();
            assert_eq!(reader.get(b"cold/00").await.unwrap().unwrap().len(), 16384);
            assert!(reader.has(b"cold/01").await.unwrap());
            assert_eq!(reader.get_size(b"cold/02").await.unwrap(), Some(16384));
            let mut scan = reader
                .iterator(crate::corekv::IterOptions::new().with_prefix(b"cold/".to_vec()))
                .await
                .unwrap();
            let mut seen = 0;
            while let Some(pair) = scan.next().await.unwrap() {
                assert_eq!(pair.value.len(), 16384);
                seen += 1;
            }
            drop(scan);
            reader.discard();

            let mut writer = on_worker.new_txn(false).await.unwrap();
            let mut scan = writer
                .iterator(crate::corekv::IterOptions::new().with_prefix(b"cold/".to_vec()))
                .await
                .unwrap();
            let mut seen_writing = 0;
            while scan.next().await.unwrap().is_some() {
                seen_writing += 1;
            }
            drop(scan);
            writer.set(b"result", b"done").await.unwrap();
            writer.commit().await.unwrap();
            (seen, seen_writing)
        }),
    )
    .unwrap()
    .unwrap();
    assert_eq!(done.wait_timeout(DEADLINE).unwrap(), (24, 24));
    pool.shutdown_blocking().unwrap();
}

/// Commit validation on a worker must be able to finish when a concurrent
/// commit advances its snapshot. The transaction's blind put targets a key in
/// a flushed table whose data block has not been read into the cache.
#[test]
fn explicit_commits_complete_after_validation_cache_misses() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    const SEED_COUNT: usize = 24;
    const VALUE_SIZE: usize = 16 * 1024;

    let dir = tempfile::tempdir().unwrap();
    let store = RegolithStore::open(dir.path()).unwrap();
    workers::block_on(async {
        let mut seed = store.new_txn(false).await.unwrap();
        let mut state = 0x9e37_79b9u32;
        for n in 0..SEED_COUNT {
            let mut value = vec![0; VALUE_SIZE];
            for byte in &mut value {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                *byte = state as u8;
            }
            seed.set(format!("cold/{n:04}").as_bytes(), &value)
                .await
                .unwrap();
        }
        seed.commit().await.unwrap();
        store.engine().db().flush().unwrap();
        store.close().await.unwrap();
    })
    .unwrap();
    drop(store);

    let pool = pool(2);
    let mut options = RegolithStoreOptions::default();
    // No table blocks are admitted; the test reaches validation with the
    // seeded SST data cold in the cache.
    options.engine = options.engine.block_cache_size(0);
    let options = options.placed_on(&pool);
    let store = RegolithStore::open_with_options(dir.path(), options).unwrap();
    let committed_callbacks = Arc::new(AtomicUsize::new(0));
    let on_worker = store.clone();
    let callbacks = Arc::clone(&committed_callbacks);
    let done = workers::block_on(
        pool.submit(workers::Target::LeastLoaded, move || async move {
            let mut warm = on_worker.new_txn(false).await.unwrap();
            warm.set(b"warm", &u32::MAX.to_be_bytes()).await.unwrap();
            warm.commit().await.unwrap();

            for n in 0..SEED_COUNT {
                let key = format!("cold/{n:04}");
                let mut stale = on_worker.new_txn(false).await.unwrap();
                stale
                    .set(key.as_bytes(), &vec![n as u8; VALUE_SIZE])
                    .await
                    .unwrap();

                // This key stays in the memtable, so B commits without I/O.
                // Its commit advances the horizon after A's blind put.
                let mut winner = on_worker.new_txn(false).await.unwrap();
                let winner_key = b"warm";
                winner.set(winner_key, &n.to_be_bytes()).await.unwrap();
                let winner_callbacks = Arc::clone(&callbacks);
                winner
                    .on_commit(OnCommit::Sync(Box::new(move || {
                        winner_callbacks.fetch_add(1, Ordering::SeqCst);
                    })))
                    .unwrap();
                winner.commit().await.unwrap();

                // A now validates its cold flushed write key against the
                // table before entering the commit pipeline.
                let stale_callbacks = Arc::clone(&callbacks);
                stale
                    .on_commit(OnCommit::Sync(Box::new(move || {
                        stale_callbacks.fetch_add(1, Ordering::SeqCst);
                    })))
                    .unwrap();
                stale.commit().await.unwrap();
            }
        }),
    )
    .unwrap()
    .unwrap();
    done.wait_timeout(DEADLINE).unwrap();
    assert_eq!(committed_callbacks.load(Ordering::SeqCst), SEED_COUNT * 2);

    workers::block_on(store.close()).unwrap().unwrap();
    drop(store);
    let reopened = RegolithStore::open(dir.path()).unwrap();
    let reader = workers::block_on(reopened.new_txn(true)).unwrap().unwrap();
    for n in 0..SEED_COUNT {
        let key = format!("cold/{n:04}");
        assert_eq!(
            workers::block_on(reader.get(key.as_bytes()))
                .unwrap()
                .unwrap()
                .unwrap()
                .as_ref(),
            vec![n as u8; VALUE_SIZE].as_slice()
        );
    }
    assert_eq!(
        workers::block_on(reader.get(b"warm"))
            .unwrap()
            .unwrap()
            .unwrap()
            .as_ref(),
        (SEED_COUNT - 1).to_be_bytes().as_slice()
    );
    reader.discard();
    workers::block_on(reopened.close()).unwrap().unwrap();
    pool.shutdown_blocking().unwrap();
}

/// Durable commits on one worker interleave: each task awaits its ticket
/// while the worker runs the others, and the worker's own queue completes
/// every ticket. A worker that blocked in a commit would finish them one at
/// a time; one that waited on its own completion would never finish.
#[test]
fn durable_commits_on_one_worker_complete_through_tickets() {
    let dir = tempfile::tempdir().unwrap();
    let pool = pool(1);
    let store = RegolithStore::open_with_options(
        dir.path(),
        RegolithStoreOptions::default().placed_on(&pool),
    )
    .unwrap();
    let on_worker = store.clone();
    let done = workers::block_on(pool.submit(workers::Target::Worker(0), move || async move {
        let commits: Vec<_> = (0..32u32)
            .map(|n| {
                let store = on_worker.clone();
                workers::spawn_local(async move {
                    let mut txn = store.new_txn(false).await.unwrap();
                    txn.set(&n.to_be_bytes(), b"v").await.unwrap();
                    txn.commit().await.unwrap();
                })
                .unwrap()
            })
            .collect();
        for commit in commits {
            commit.await.unwrap();
        }
        let range = on_worker.allocate(b"ids", 3).await.unwrap();
        range.end - range.start
    }))
    .unwrap()
    .unwrap();
    assert_eq!(done.wait_timeout(DEADLINE).unwrap(), 3);
    let reader = workers::block_on(store.new_txn(true)).unwrap().unwrap();
    for n in 0..32u32 {
        let value = workers::block_on(reader.get(&n.to_be_bytes()))
            .unwrap()
            .unwrap();
        assert_eq!(value.as_deref(), Some(&b"v"[..]));
    }
    pool.shutdown_blocking().unwrap();
}

/// A worker lets go of a database's queue once the database is gone.
#[test]
fn a_dropped_database_leaves_no_queue_behind() {
    let pool = pool(1);
    let store = RegolithStore::in_memory().unwrap();
    let on_worker = store.clone();
    let opened = pool
        .submit_and_wait(workers::Target::Worker(0), move || async move {
            super::queue_for(on_worker.engine_arc()).map(|_| super::registry::len())
        })
        .unwrap();
    assert_eq!(opened, Some(1));
    drop(store);
    let left = pool
        .submit_and_wait(workers::Target::Worker(0), || async {
            // The hook ran between this task's admission and its first poll.
            workers::yield_now().await;
            super::registry::len()
        })
        .unwrap();
    assert_eq!(left, 0);
    pool.shutdown_blocking().unwrap();
}

/// A call off the workers of a store's pool borrows a worker's queue: a cold
/// read and a durable commit made from a plain thread complete without that
/// thread reading the device or waiting for a sync, and each such call is
/// counted as one that was not routed to the pool.
#[test]
fn a_call_off_the_pool_borrows_a_workers_queue_and_is_counted() {
    let dir = tempfile::tempdir().unwrap();
    let pool = pool(2);
    let store = cold_store(dir.path(), 4, &pool);
    let stats = store.transaction_stats_handle().unwrap();
    assert_eq!(stats.snapshot().off_pool_calls, 0);

    workers::block_on(async {
        let reader = store.new_txn(true).await.unwrap();
        assert_eq!(reader.get(b"cold/00").await.unwrap().unwrap().len(), 16384);
        reader.discard();
        let mut writer = store.new_txn(false).await.unwrap();
        writer.set(b"off-pool", b"v").await.unwrap();
        writer.commit().await.unwrap();
    })
    .unwrap();
    assert_eq!(stats.snapshot().off_pool_calls, 2);

    // A call routed to the pool is not counted.
    let on_worker = store.clone();
    pool.submit_and_wait(workers::Target::LeastLoaded, move || async move {
        on_worker.new_txn(true).await.unwrap().discard();
    })
    .unwrap();
    assert_eq!(stats.snapshot().off_pool_calls, 2);
    pool.shutdown_blocking().unwrap();
}

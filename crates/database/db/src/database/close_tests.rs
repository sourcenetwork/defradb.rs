//! A database closes in the one shutdown order: the work its pool admitted
//! finishes and the pool stops, then the store closes.

use std::sync::Arc;
use std::time::Duration;

use regolith::sync::Event;
use storage::corekv::{Store, Writer};
use storage::{RegolithStore, RegolithStoreOptions};

use crate::database::{DbOptions, DB};

const DEADLINE: Duration = Duration::from_secs(60);

/// A write still running on the pool when close begins commits: the store
/// closes only once the pool has stopped. Closing the store first would fail
/// that commit, or close with the transaction still in flight.
#[test]
fn close_stops_the_pool_before_it_closes_the_store() {
    let pool = workers::Pool::start(storage::with_worker_io(workers::Config::unplaced(1))).unwrap();
    let store = RegolithStore::open_with_options(
        "close-order",
        RegolithStoreOptions::memory().placed_on(&pool),
    )
    .unwrap();
    let db =
        Arc::new(DB::with_options(store, DbOptions::default().with_pool(pool.clone())).unwrap());

    let (started, release) = (Arc::new(Event::new()), Arc::new(Event::new()));
    let (running, held) = (Arc::clone(&started), Arc::clone(&release));
    let writer = Arc::clone(&db);
    let write = workers::block_on(pool.submit(workers::Target::Worker(0), move || async move {
        let mut txn = writer.store().new_txn(false).await?;
        txn.set(b"key", b"value").await?;
        running.set();
        held.wait().await;
        txn.commit().await
    }))
    .unwrap()
    .unwrap();
    workers::block_on_timeout(started.wait(), DEADLINE).unwrap();

    let closer = Arc::clone(&db);
    let close = std::thread::spawn(move || workers::block_on(closer.close()).unwrap());
    release.set();

    assert!(close.join().unwrap().is_ok(), "the store closed cleanly");
    assert!(
        write.wait_timeout(DEADLINE).unwrap().is_ok(),
        "the write committed"
    );
    assert!(
        workers::block_on_timeout(pool.stopped(), DEADLINE).is_ok(),
        "the pool stopped"
    );
    assert!(db.is_closed());
}

//! Reads racing an engine's commits, and its close, add every count to the
//! exported total exactly once, and no read sees the total go back.
//!
//! A test binary of its own: the totals are process-wide, and this one asserts
//! them exactly.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use regolith::{OptimisticTransactionDb, Options, Statistics, Ticker, TxnOptions};

fn exported_commits() -> u64 {
    telemetry::engine_ticker_totals()
        .into_iter()
        .find(|(name, _)| *name == Ticker::CommitCount.name())
        .map_or(0, |(_, total)| total)
}

#[test]
fn racing_reads_add_every_commit_once() {
    const COMMITS: u64 = 20_000;
    const READERS: usize = 4;
    let statistics = Arc::new(Statistics::new());
    let options = Options::memory().statistics(Some(Arc::clone(&statistics)));
    let db = OptimisticTransactionDb::open("engine-tickers-race", options).unwrap();
    let export = telemetry::export_engine_tickers(&statistics);
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        for _ in 0..READERS {
            scope.spawn(|| {
                let mut last = 0;
                while !done.load(Ordering::Acquire) {
                    let total = exported_commits();
                    assert!(total >= last, "the total went back: {total} < {last}");
                    assert!(total <= COMMITS, "{total} commits exported of {COMMITS}");
                    last = total;
                }
            });
        }
        for commit in 0..COMMITS {
            let txn = db.begin(&TxnOptions::new());
            txn.put(&commit.to_be_bytes(), b"value").unwrap();
            txn.commit().unwrap();
        }
        done.store(true, Ordering::Release);
    });
    drop(export);
    assert_eq!(exported_commits(), COMMITS);
}

use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc,
};
use storage::backends::test_suite::on_workers;
use storage::corekv::{
    Error, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryOptions,
    RetryTxn, Store, TxnCallbacks, Writer,
};
use storage::{RegolithStore, RegolithStoreOptions};

#[derive(Default)]
struct CallerListener(AtomicU64);

impl regolith::EventListener for CallerListener {
    fn on_conflict(&self, _: &regolith::Conflict) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn engine_counters_and_conflict_reasons_preserve_caller_instrumentation() {
    on_workers(|| async {
        let engine = Arc::new(regolith::Statistics::default());
        let listener = Arc::new(CallerListener::default());
        let mut options = RegolithStoreOptions::memory();
        options.engine = options
            .engine
            .statistics(Some(engine.clone()))
            .add_listener(listener.clone());
        let store = RegolithStore::open_with_options("telemetry", options).unwrap();
        let stats = store.transaction_stats_handle().unwrap();
        let mut seed = store.new_txn(false).await.unwrap();
        seed.set(b"ordinary", b"before").await.unwrap();
        seed.commit().await.unwrap();
        let mut reader = store.new_txn(false).await.unwrap();
        reader.get(b"ordinary").await.unwrap();
        reader.set(b"effect", b"yes").await.unwrap();
        let mut delete = store.new_txn(false).await.unwrap();
        delete.delete(b"ordinary").await.unwrap();
        delete.commit().await.unwrap();
        assert!(reader.commit().await.unwrap_err().is_txn_conflict());

        let snapshot = stats.snapshot();
        assert_eq!(
            snapshot.commits,
            engine.get_ticker(regolith::Ticker::CommitCount)
        );
        assert_eq!(snapshot.conflicts, 1);
        assert_eq!(snapshot.conflict_reasons.get("read/delete"), Some(&1));
        assert_eq!(snapshot.engine["regolith.commit.conflicts_after_delete"], 1);
        assert_eq!(listener.0.load(Ordering::Relaxed), 1);
        assert_eq!(snapshot.engine.len(), 8);

        // A read-only adapter transaction owns a snapshot, not a native commit.
        store.new_txn(true).await.unwrap().commit().await.unwrap();
        assert_eq!(stats.snapshot().commits, snapshot.commits);
        let other = RegolithStore::in_memory().unwrap();
        assert_eq!(
            other
                .transaction_stats_handle()
                .unwrap()
                .snapshot()
                .conflicts,
            0
        );
    });
}

#[test]
fn accepted_blind_merges_are_visible_in_policy_tickers() {
    on_workers(|| async {
        let store = RegolithStore::in_memory().unwrap();
        let mut seed = store.new_txn(false).await.unwrap();
        seed.set(b"counter", &0_i64.to_be_bytes()).await.unwrap();
        seed.commit().await.unwrap();
        let mut first = store.new_txn(false).await.unwrap();
        let mut second = store.new_txn(false).await.unwrap();
        let delta = storage::merge::counter_i64(1);
        Writer::merge(&mut first, b"counter", &delta).await.unwrap();
        Writer::merge(&mut second, b"counter", &delta)
            .await
            .unwrap();
        first.commit().await.unwrap();
        second.commit().await.unwrap();
        let stats = store.transaction_stats_handle().unwrap().snapshot();
        assert_eq!(stats.conflicts, 0);
        assert_eq!(stats.engine["regolith.policy.blind_merges_commuted"], 1);
    });
}

fn exported_total(name: &str) -> u64 {
    telemetry::engine_ticker_totals()
        .into_iter()
        .find(|(ticker, _)| *ticker == name)
        .map_or(0, |(_, total)| total)
}

/// A store's commit and policy tickers reach the OpenTelemetry export, the same
/// list its snapshot reports, and a store that closes leaves what it counted in
/// the exported totals. Other tests commit concurrently, so totals only grow.
#[test]
fn engine_tickers_are_exported_and_kept_when_the_store_closes() {
    on_workers(|| async {
        let before = exported_total("regolith.commit.count");
        let store = RegolithStore::in_memory().unwrap();
        let reported: Vec<&str> = store
            .transaction_stats_handle()
            .unwrap()
            .snapshot()
            .engine
            .into_keys()
            .collect();
        let mut exported: Vec<&str> = telemetry::engine_ticker_totals()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        exported.sort_unstable();
        assert_eq!(reported, exported);

        for _ in 0..3 {
            let mut txn = store.new_txn(false).await.unwrap();
            txn.set(b"exported", b"value").await.unwrap();
            txn.commit().await.unwrap();
        }
        let counted = store.transaction_stats_handle().unwrap().snapshot().commits;
        assert!(counted >= 3, "{counted}");
        let live = exported_total("regolith.commit.count");
        assert!(live >= before + counted, "{live} < {before} + {counted}");

        drop(store);
        assert!(exported_total("regolith.commit.count") >= live);
    });
}

struct ConflictOnceAttempt {
    trigger_writer: Option<regolith::sync::BoundedSender<()>>,
    writer_finished: Option<regolith::sync::BoundedReceiver<()>>,
    attempts: Arc<AtomicUsize>,
}

impl RetryAttemptFn for ConflictOnceAttempt {
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            let mut io = retry.begin_attempt_io()?;
            io.get(b"retry-counter").await?;
            if let Some(trigger) = self.trigger_writer.take() {
                let _ = trigger.try_send(());
                self.writer_finished
                    .take()
                    .expect("first attempt owns writer notification")
                    .recv_async()
                    .await
                    .ok_or_else(|| {
                        RetryAttemptError::Storage(Error::Other(
                            "conflict writer stopped before commit".into(),
                        ))
                    })?;
            }
            io.set(b"retry-counter", b"attempt").await?;
            retry.finish_attempt_io(io, TxnCallbacks::default())?;
            Ok(Box::new(()) as RetryAttemptValue)
        })
    }
}

#[test]
fn retry_adapter_counts_native_conflicts_and_commits_once() {
    on_workers(|| async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut seed = store.new_txn(false).await.unwrap();
        seed.set(b"retry-counter", b"seed").await.unwrap();
        seed.commit().await.unwrap();
        let before = store.transaction_stats_handle().unwrap().snapshot();

        let writer_store = Arc::clone(&store);
        let (trigger_writer, start_writer) = regolith::sync::bounded(1);
        let (writer_finished, finish_writer) = regolith::sync::bounded(1);
        let writer = workers::spawn(None, "test", move || async move {
            start_writer.recv_async().await.unwrap();
            let mut txn = writer_store.new_txn(false).await.unwrap();
            txn.set(b"retry-counter", b"concurrent").await.unwrap();
            txn.commit().await.unwrap();
            let _ = writer_finished.try_send(());
        })
        .unwrap();

        let attempts = Arc::new(AtomicUsize::new(0));
        store
            .transact_retry(
                RetryOptions::new(3),
                Box::new(ConflictOnceAttempt {
                    trigger_writer: Some(trigger_writer),
                    writer_finished: Some(finish_writer),
                    attempts: Arc::clone(&attempts),
                }),
            )
            .await
            .unwrap();
        writer.await.unwrap();

        let after = store.transaction_stats_handle().unwrap().snapshot();
        assert_eq!(attempts.load(Ordering::Relaxed), 2);
        assert_eq!(after.commits - before.commits, 2); // writer and winning attempt
        assert_eq!(after.conflicts - before.conflicts, 1);
        assert_eq!(after.conflict_reasons.get("read/put"), Some(&1));
    });
}

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use storage::corekv::{
    Reader, RetryAttemptError, RetryAttemptFuture, RetryAttemptValue, RetryOptions, RetryTxn,
    Store, Writer,
};
use storage::RegolithStore;

struct Attempt {
    store: RegolithStore,
    runs: Arc<AtomicUsize>,
    conflicts_to_create: usize,
    conflict_prefix: &'static str,
}

impl storage::corekv::RetryAttemptFn for Attempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let run = self.runs.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = txn.get(b"counter").await?;
            txn.set(b"counter", b"engine-attempt").await?;

            if run <= self.conflicts_to_create {
                // Commit a competing write only after the engine attempt has
                // read and staged its write, forcing its commit to replay.
                let mut competitor = self.store.new_txn(false).await?;
                let _ = competitor.get(b"counter").await?;
                let competing_value = format!("{}-{run}", self.conflict_prefix);
                competitor
                    .set(b"counter", competing_value.as_bytes())
                    .await?;
                competitor.commit().await?;
            }

            Ok(Box::new(()) as RetryAttemptValue)
        })
    }
}

#[test]
fn engine_retry_counter_counts_only_actual_replay_executions() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut seed = store.new_txn(false).await.unwrap();
        seed.set(b"counter", b"initial").await.unwrap();
        seed.commit().await.unwrap();

        let before = telemetry::conflict_metrics_snapshot().engine.attempts;
        let no_conflict_runs = Arc::new(AtomicUsize::new(0));
        let no_conflict = Attempt {
            store: store.clone(),
            runs: no_conflict_runs.clone(),
            conflicts_to_create: 0,
            conflict_prefix: "unused",
        };
        store
            .transact_retry(RetryOptions::new(3), Box::new(no_conflict))
            .await
            .unwrap();
        assert_eq!(no_conflict_runs.load(Ordering::Relaxed), 1);
        let after_no_conflict = telemetry::conflict_metrics_snapshot().engine.attempts;
        assert_eq!(after_no_conflict - before, 0);

        let one_conflict_runs = Arc::new(AtomicUsize::new(0));
        let one_conflict = Attempt {
            store: store.clone(),
            runs: one_conflict_runs.clone(),
            conflicts_to_create: 1,
            conflict_prefix: "single-conflict",
        };
        store
            .transact_retry(RetryOptions::new(3), Box::new(one_conflict))
            .await
            .unwrap();
        assert_eq!(one_conflict_runs.load(Ordering::Relaxed), 2);
        let after_successful_replay = telemetry::conflict_metrics_snapshot().engine.attempts;
        assert_eq!(after_successful_replay - after_no_conflict, 1);

        let exhausted_runs = Arc::new(AtomicUsize::new(0));
        let every_attempt_conflicts = Attempt {
            store: store.clone(),
            runs: exhausted_runs.clone(),
            conflicts_to_create: usize::MAX,
            conflict_prefix: "exhaustion",
        };
        let result = store
            .transact_retry(RetryOptions::new(3), Box::new(every_attempt_conflicts))
            .await;
        assert!(matches!(result, Err(RetryAttemptError::Exhausted(_))));
        assert_eq!(exhausted_runs.load(Ordering::Relaxed), 3);
        let after_exhaustion = telemetry::conflict_metrics_snapshot().engine.attempts;
        assert_eq!(after_exhaustion - after_successful_replay, 2);
    })
    .unwrap()
}

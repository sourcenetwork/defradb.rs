//! Running an attempt as one engine transaction, the one way every caller
//! does it.
//!
//! [`transact`] hands an attempt to [`Store::transact_retry`], which re-runs
//! it after a conflict under the caller's [`RetryOptions`] and commits the run
//! that wins, and returns that run's value as the caller's own type. A caller
//! that reports its retries as a layer of its own names the layer: its
//! re-runs, its retried sequences that commit and those that run out of
//! budget are counted there, on top of the engine's own count.
//!
//! ```ignore
//! let added: usize = transact(&store, retry, None, Box::new(attempt)).await?;
//! ```

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use telemetry::RetryLayer;

use super::{
    Error, RetryAttempt, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryOptions,
    RetryTxn, Store,
};

/// Runs `attempt` as one engine transaction of `store` and returns the value
/// of the run that committed. Only a commit conflict re-runs it; an error the
/// attempt returns ends the transaction with that error.
pub async fn transact<S, T>(
    store: &S,
    options: RetryOptions,
    layer: Option<RetryLayer>,
    attempt: RetryAttempt,
) -> Result<T, RetryAttemptError>
where
    S: Store + ?Sized,
    T: Any,
{
    let retried = Arc::new(AtomicBool::new(false));
    let attempt: RetryAttempt = match layer {
        Some(layer) => Box::new(Counted {
            inner: attempt,
            layer,
            retried: Arc::clone(&retried),
        }),
        None => attempt,
    };
    let outcome = store.transact_retry(options, attempt).await;
    if let Some(layer) = layer {
        match &outcome {
            Err(RetryAttemptError::Exhausted(_)) => telemetry::record_retry_exhaustion(layer),
            Ok(_) if retried.load(Ordering::Relaxed) => telemetry::record_retry_success(layer),
            _ => {}
        }
    }
    outcome?.downcast::<T>().map(|value| *value).map_err(|_| {
        RetryAttemptError::Storage(Error::Other(
            "a transaction attempt returned a value of another type".into(),
        ))
    })
}

/// An attempt whose re-runs after a conflict count in a retry layer.
struct Counted {
    inner: RetryAttempt,
    layer: RetryLayer,
    retried: Arc<AtomicBool>,
}

impl RetryAttemptFn for Counted {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        if txn.previous_conflict().is_some() {
            telemetry::record_retry_attempt(self.layer);
            self.retried.store(true, Ordering::Relaxed);
        }
        self.inner.run(txn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::RegolithStore;
    use crate::corekv::{engine_retry_options, RetryAttemptValue};
    use std::sync::atomic::AtomicUsize;

    /// Writes `key`; its first run loses to a write of the key it read.
    struct LosesOnce {
        store: Arc<RegolithStore>,
        runs: Arc<AtomicUsize>,
    }

    impl RetryAttemptFn for LosesOnce {
        fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
            Box::pin(async move {
                let _ = txn.get(b"transact-key").await?;
                txn.set(b"transact-key", b"mine").await?;
                if self.runs.fetch_add(1, Ordering::SeqCst) == 0 {
                    let mut winner = self.store.new_txn(false).await?;
                    winner.set(b"transact-key", b"theirs").await?;
                    winner.commit().await?;
                }
                Ok(Box::new(7usize) as RetryAttemptValue)
            })
        }
    }

    fn loses_once(store: &Arc<RegolithStore>) -> (RetryAttempt, Arc<AtomicUsize>) {
        let runs = Arc::new(AtomicUsize::new(0));
        let attempt = Box::new(LosesOnce {
            store: Arc::clone(store),
            runs: Arc::clone(&runs),
        });
        (attempt, runs)
    }

    #[test]
    fn the_committed_run_returns_its_value_as_the_callers_type() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let (attempt, runs) = loses_once(&store);
            let value: usize = transact(&*store, engine_retry_options(1), None, attempt)
                .await
                .unwrap();
            assert_eq!(value, 7);
            assert_eq!(runs.load(Ordering::SeqCst), 2);
        })
        .unwrap()
    }

    #[test]
    fn a_value_of_another_type_is_an_error_not_a_panic() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let (attempt, _) = loses_once(&store);
            let error = transact::<_, String>(&*store, engine_retry_options(1), None, attempt)
                .await
                .unwrap_err();
            assert!(matches!(error, RetryAttemptError::Storage(Error::Other(_))));
        })
        .unwrap()
    }

    /// The counters are process-wide, so the test reads how much each grew.
    #[test]
    fn a_named_layer_counts_re_runs_successes_and_exhaustions() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let before = telemetry::conflict_metrics_snapshot().push_marker;

            let (attempt, _) = loses_once(&store);
            let _: usize = transact(
                &*store,
                engine_retry_options(1),
                Some(RetryLayer::PushMarker),
                attempt,
            )
            .await
            .unwrap();
            let retried = telemetry::conflict_metrics_snapshot().push_marker;
            assert!(retried.attempts > before.attempts, "{retried:?}");
            assert!(retried.successes > before.successes, "{retried:?}");

            let (attempt, _) = loses_once(&store);
            let error = transact::<_, usize>(
                &*store,
                engine_retry_options(0),
                Some(RetryLayer::PushMarker),
                attempt,
            )
            .await
            .unwrap_err();
            assert!(matches!(error, RetryAttemptError::Exhausted(_)));
            let exhausted = telemetry::conflict_metrics_snapshot().push_marker;
            assert!(exhausted.exhaustions > retried.exhaustions, "{exhausted:?}");
        })
        .unwrap()
    }
}

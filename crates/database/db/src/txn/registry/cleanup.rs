//! The idle-transaction sweep: each owner is asked to end its transactions
//! that no request has used for the idle limit. The ask is a message queued
//! behind the requests already there, so a request that arrived first keeps
//! its transaction. Subject of proofs/tla/TxnRegistry.tla.

use super::*;

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// Ends every transaction no request has used for longer than
    /// `max_idle_age`, each after the requests queued for it before this, and
    /// says how many ended.
    ///
    /// Owning `TransactionGuard`s clean up on drop without waiting for a sweep.
    pub async fn cleanup_stale_transactions(
        &self,
        max_idle_age: Duration,
    ) -> Result<CleanupResult> {
        let cleaned = self
            .owners()
            .map_err(|error| Error::Other(error.to_string()))?
            .expire_idle(max_idle_age)
            .await
            .map_err(|error| Error::Other(error.to_string()))?;
        Ok(CleanupResult {
            cleaned,
            failed: Vec::new(),
        })
    }

    /// Starts the sweep that cleans up transactions idle for longer than
    /// `max_idle_age`: a background job of the database's worker pool, run
    /// every `sweep_interval` on its worker's deadline queue until it is
    /// aborted or its worker stops.
    ///
    /// # Panics
    ///
    /// Panics if `sweep_interval` is zero.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_stale_transaction_cleanup(
        self: &Arc<Self>,
        max_idle_age: Duration,
        sweep_interval: Duration,
    ) -> workers::Background {
        assert!(
            !sweep_interval.is_zero(),
            "transaction cleanup sweep interval must be non-zero"
        );

        let registry = Arc::clone(self);
        let pool = registry.db.pool().cloned();
        workers::spawn_background(pool.as_ref(), "db::transaction_sweep", move || async move {
            let Err(error) = workers::every(sweep_interval, || async {
                match registry.cleanup_stale_transactions(max_idle_age).await {
                    Ok(result) if result.attempted() > 0 && result.is_complete() => {
                        tracing::info!(
                            cleaned = result.cleaned,
                            max_idle_age_secs = max_idle_age.as_secs(),
                            "Cleaned up idle transactions"
                        );
                    }
                    Ok(result) if result.attempted() > 0 => {
                        tracing::warn!(
                            cleaned = result.cleaned,
                            failed = result.failed.len(),
                            max_idle_age_secs = max_idle_age.as_secs(),
                            "Idle transaction cleanup completed with failures"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            max_idle_age_secs = max_idle_age.as_secs(),
                            "Idle transaction cleanup failed"
                        );
                    }
                }
            })
            .await;
            tracing::error!(%error, "the transaction sweep stopped: it runs only on a worker");
        })
    }
}

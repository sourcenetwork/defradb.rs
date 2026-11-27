//! The one engine retry policy. Every `transact_retry` caller builds its
//! [`RetryOptions`] here from the operator's `datastore.max_txn_retries`, so
//! the configured budget reaches every engine retry and no caller keeps a
//! count of its own.

use super::RetryOptions;

/// Re-runs after a conflict when nothing configures them, the first run not
/// counted. The default of `datastore.max_txn_retries`.
pub const DEFAULT_MAX_TXN_RETRIES: u32 = 5;

/// The engine retry policy for a budget of `max_txn_retries` re-runs: the
/// first run plus that many. The engine re-runs at once after a conflict: a
/// race with one winner settles on the re-run, which sees the winner.
pub fn engine_retry_options(max_txn_retries: u32) -> RetryOptions {
    RetryOptions::new(max_txn_retries.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_counts_re_runs_after_the_first_run() {
        assert_eq!(engine_retry_options(0).max_attempts, 1);
        assert_eq!(engine_retry_options(5).max_attempts, 6);
        assert_eq!(engine_retry_options(u32::MAX).max_attempts, u32::MAX);
    }
}

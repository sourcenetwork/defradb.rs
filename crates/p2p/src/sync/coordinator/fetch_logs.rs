//! The DAG fetcher's no-progress, retry and exhaustion lines, each throttled
//! per dominant failure kind.

use std::fmt::Write as _;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use cid::Cid;
use tracing::{debug, error, warn};

use super::dag_context::DagFetchContext;
use super::dag_fetcher::BLOCK_SYNC_COMPLETION_WATCHDOG;
use super::dag_retry::ProviderRotation;
use super::fetch_failures::{FetchFailures, WindowFailure};
use crate::sync::manager::log_throttle::{
    DagFetchLogThrottles, ExhaustionTallies, ExhaustionTally, LogAdmission, LogThrottle,
    DAG_FETCH_LOG_SLOTS, LOG_THROTTLE_WINDOW,
};

const BUDGET_EXHAUSTED_SLOT: usize = WindowFailure::ALL.len();
const _: () = assert!(BUDGET_EXHAUSTED_SLOT + 1 == DAG_FETCH_LOG_SLOTS);

fn slot(failures: &FetchFailures) -> usize {
    failures
        .dominant()
        .map_or(BUDGET_EXHAUSTED_SLOT, |failure| failure as usize)
}

fn slot_label(slot: usize) -> &'static str {
    WindowFailure::ALL
        .get(slot)
        .map_or("budget_exhausted", |failure| failure.label())
}

/// Supersession is routine, so it always logs at debug and outside the count.
fn admit_unless_superseded(
    throttles: &[LogThrottle; DAG_FETCH_LOG_SLOTS],
    failures: &FetchFailures,
) -> LogAdmission {
    if failures.is_superseded_only() {
        LogAdmission::Suppressed
    } else {
        throttles[slot(failures)].admit()
    }
}

macro_rules! failure_line {
    ($admission:expr, $level:ident, $failures:expr, $($field:tt)+) => {
        match $admission {
            LogAdmission::Full { suppressed } => failure_line!(@ $level, $failures, suppressed, $($field)+),
            LogAdmission::Suppressed => failure_line!(@ debug, $failures, 0u64, $($field)+),
        }
    };
    (@ $level:ident, $failures:expr, $suppressed:expr, $($field:tt)+) => {
        $level!(
            provider_outcomes = %$failures.provider_outcomes(),
            timed_out = $failures.count(WindowFailure::TimedOut),
            transport_error = $failures.count(WindowFailure::Error),
            send_failed = $failures.count(WindowFailure::SendFailed),
            no_blocks = $failures.count(WindowFailure::NoBlocks),
            unservable = $failures.count(WindowFailure::Unservable),
            refused = $failures.count(WindowFailure::Refused),
            busy = $failures.count(WindowFailure::Busy),
            superseded = $failures.count(WindowFailure::Superseded),
            budget_exhausted = $failures.budget_exhausted,
            suppressed_since_last = $suppressed,
            $($field)+
        )
    };
}

pub(super) fn log_no_progress_batch(
    root_cid: &Cid,
    requested_count: usize,
    failures: &FetchFailures,
    logs: &DagFetchLogThrottles,
) {
    failure_line!(
        admit_unless_superseded(&logs.no_progress_batch, failures),
        warn,
        failures,
        root_cid = %root_cid,
        requested_count,
        watchdog_secs = BLOCK_SYNC_COMPLETION_WATCHDOG.as_secs(),
        "Selective block batch made no progress: {}",
        failures.batch_reason()
    );
}

pub(super) fn log_retry(
    root_cid: &Cid,
    context: &DagFetchContext,
    attempt: u32,
    remaining_count: usize,
    backoff: Duration,
    failures: &FetchFailures,
    logs: &DagFetchLogThrottles,
) {
    failure_line!(
        admit_unless_superseded(&logs.retry, failures),
        warn,
        failures,
        root_cid = %root_cid,
        doc_id = %context.doc_id,
        attempt,
        remaining_count,
        backoff_ms = backoff.as_millis() as u64,
        "DAG fetch incomplete, retrying after backoff"
    );
}

pub(super) fn log_exhausted(
    root_cid: &Cid,
    context: &DagFetchContext,
    remaining_count: usize,
    attempts: u32,
    providers: &ProviderRotation,
    failures: &FetchFailures,
    logs: &DagFetchLogThrottles,
) {
    let slot = slot(failures);
    let admission = logs.exhausted[slot].admit();
    count_toward_summary(
        logs.network(),
        &logs.exhausted_by_providers,
        providers,
        slot,
        matches!(admission, LogAdmission::Full { .. }),
    );
    // Exhaustion does not abandon the root: the per-root retry clock owns
    // convergence from here, and any provider reconnect expedites it.
    failure_line!(
        admission,
        error,
        failures,
        root_cid = %root_cid,
        doc_id = %context.doc_id,
        collection_id = %context.collection_id,
        remaining_count,
        attempts,
        provider_count = providers.len(),
        providers = ?providers.peers(),
        "DAG fetch failed after exhausting retries and providers"
    );
}

/// The first exhaustion against a provider set with nothing pending schedules
/// that set's summary line for when the window closes.
fn count_toward_summary(
    network: Option<&crate::network::Network>,
    tallies: &ExhaustionTallies,
    providers: &ProviderRotation,
    slot: usize,
    logged_full: bool,
) {
    let mut peers: Vec<_> = providers.peers().iter().map(ToString::to_string).collect();
    peers.sort_unstable();
    let key = peers.join(",");
    let tally = tallies.get_or_insert(key.clone(), Arc::default());
    if tally.pending.fetch_add(1, Ordering::AcqRel) == 0 {
        if let Some(network) = network {
            network.spawn(log_exhaustion_summary(
                Arc::clone(tallies),
                key,
                Arc::clone(&tally),
            ));
        }
    }
    tally.roots[slot].fetch_add(1, Ordering::AcqRel);
    if logged_full {
        tally.logged_full.fetch_add(1, Ordering::AcqRel);
    }
}

/// Runs until every root counted on `tally` has been reported, so a root
/// counted while a window drains lands in the next window, never nowhere.
async fn log_exhaustion_summary(
    tallies: ExhaustionTallies,
    providers: String,
    tally: Arc<ExhaustionTally>,
) {
    loop {
        crate::rt::time::sleep(LOG_THROTTLE_WINDOW).await;
        let logged_full = tally.logged_full.swap(0, Ordering::AcqRel);
        let (mut total, mut breakdown) = (0, String::new());
        for (slot, roots) in tally.roots.iter().enumerate() {
            let count = roots.swap(0, Ordering::AcqRel);
            if count > 0 {
                total += count;
                let _ = write!(breakdown, " {}={count}", slot_label(slot));
            }
        }
        // Skipped only for a lone root whose own line already went out at
        // full level; a demoted lone root would otherwise never reach ERROR.
        if total > 1 || (total == 1 && logged_full == 0) {
            error!(
                providers = %providers,
                roots = total,
                "{total} roots exhausted against {providers} in last {}s:{breakdown}",
                LOG_THROTTLE_WINDOW.as_secs()
            );
        }
        if tally.pending.fetch_sub(total, Ordering::AcqRel) == total {
            if tallies
                .get(&providers)
                .is_some_and(|current| Arc::ptr_eq(&current, &tally))
            {
                tallies.remove(&providers);
            }
            return;
        }
    }
}

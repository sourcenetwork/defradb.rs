//! The task that submits, by itself, what a node owes a remote document ACP.

use std::sync::Arc;
use std::time::Duration;

use storage::corekv::Store;

use super::ExternalDocumentAcp;
use crate::OwedScope;

const FIRST_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// The wait after a pass that should be tried again: twice the last, from
/// one second up to one minute.
#[derive(Debug, Default)]
struct Backoff(Option<Duration>);

impl Backoff {
    fn next(&mut self) -> Duration {
        let next = self
            .0
            .map_or(FIRST_BACKOFF, |last| (last * 2).min(MAX_BACKOFF));
        self.0 = Some(next);
        next
    }

    fn reset(&mut self) {
        self.0 = None;
    }
}

/// Run passes over everything `gate` owes until the task is stopped. A pass
/// that leaves work waits for a commit, with a back-off while the provider is
/// away or a retry can run at once.
pub async fn drain_owed_changes<S>(gate: Arc<ExternalDocumentAcp<S>>)
where
    S: Store + 'static,
{
    let mut backoff = Backoff::default();
    let mut first = true;
    loop {
        let retry = match gate.settle_owed_changes_once(OwedScope::All).await {
            Ok(settlement) => {
                if first && settlement != Default::default() {
                    tracing::info!(
                        settled = settlement.settled,
                        refused = settlement.refused,
                        discarded = settlement.discarded,
                        failed = settlement.failed,
                        awaiting_authority = settlement.awaiting_authority,
                        left = settlement.left,
                        retry = settlement.retry,
                        provider_unavailable = settlement.provider_unavailable,
                        "Submitted ACP relationship changes owed from a previous run"
                    );
                }
                settlement.wants_another_pass()
            }
            Err(error) if is_terminal(&error) => {
                tracing::error!(
                    %error,
                    "Owed ACP relationship changes are in a layout this version cannot read; the drainer stops"
                );
                return;
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    "Could not read owed ACP relationship changes; they stay owed"
                );
                true
            }
        };
        first = false;
        if retry {
            let wait = backoff.next();
            tracing::debug!(?wait, "Owed ACP relationship changes wait for the provider");
            let _ = workers::timeout(wait, gate.staged_changes_committed()).await;
        } else {
            backoff.reset();
            gate.staged_changes_committed().await;
        }
    }
}

fn is_terminal(error: &crate::Error) -> bool {
    matches!(error, crate::Error::LegacyOwedChanges { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_earlier_layout_stops_the_drainer_and_other_errors_do_not() {
        assert!(is_terminal(&crate::Error::LegacyOwedChanges { count: 1 }));
        assert!(!is_terminal(&crate::Error::Storage("busy".into())));
    }

    #[test]
    fn the_back_off_doubles_up_to_its_cap_and_starts_over_after_a_reset() {
        let mut backoff = Backoff::default();
        let waits: Vec<u64> = (0..8).map(|_| backoff.next().as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60]);
        backoff.reset();
        assert_eq!(backoff.next(), FIRST_BACKOFF);
    }
}

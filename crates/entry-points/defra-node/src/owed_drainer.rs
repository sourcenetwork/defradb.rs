//! The node's drainer: the one task that submits, by itself, what the node
//! owes a remote ACP provider.
//!
//! It starts where the redrive of owed registrations starts and runs until
//! the node stops. Each turn is one pass over everything owed, in sequence
//! order, through the gate's single pass lock
//! ([`acp::ExternalDocumentAcp::settle_owed_changes_once`]), so it is serial
//! with every other pass and with the worker behind the provider. Between
//! turns it waits for a commit that staged changes; while the provider is
//! away, answers with errors that may pass, or a pass leaves a change the
//! node may send again at once, it also turns after a back-off that doubles
//! up to [`MAX_BACKOFF`] ([`acp::OwedSettlement::wants_another_pass`]). A
//! staged change on an idle node is therefore settled without another
//! request. A parked change is not a reason to turn: only its actor moves
//! it.
//!
//! This is the fairness `SpecLiveJournaled` of `proofs/tla/AcpOutbox.tla`
//! promises: the drainer takes its turns by itself.

use std::sync::Arc;
use std::time::Duration;

use storage::corekv::Store;

/// The first wait after a pass that could not reach the provider.
pub(crate) const FIRST_BACKOFF: Duration = Duration::from_secs(1);

/// The longest wait between passes while the provider stays away.
pub(crate) const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// How long the drainer waits before its next turn.
#[derive(Debug, Default)]
pub(crate) struct Backoff(Option<Duration>);

impl Backoff {
    /// The wait after a pass that should be tried again: twice the last,
    /// from [`FIRST_BACKOFF`] up to [`MAX_BACKOFF`].
    pub(crate) fn next(&mut self) -> Duration {
        let next = self
            .0
            .map_or(FIRST_BACKOFF, |last| (last * 2).min(MAX_BACKOFF));
        self.0 = Some(next);
        next
    }

    /// A pass that left nothing to try again: the next one waits for a
    /// commit, and a later back-off starts from the first wait again.
    pub(crate) fn reset(&mut self) {
        self.0 = None;
    }
}

/// Run the drainer over `gate` until the task is stopped.
pub(crate) async fn drain<S>(gate: Arc<acp::ExternalDocumentAcp<S>>)
where
    S: Store + 'static,
{
    let mut backoff = Backoff::default();
    let mut first = true;
    loop {
        let retry = match gate.settle_owed_changes_once(acp::OwedScope::All).await {
            Ok(settlement) => {
                if first && settlement != acp::OwedSettlement::default() {
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

/// Whether `error` from a pass will not pass by itself: the store owes
/// changes in a layout this version cannot read. Another pass would only
/// meet it again, so the drainer stops; the node refuses such a store at
/// start.
fn is_terminal(error: &acp::Error) -> bool {
    matches!(error, acp::Error::LegacyOwedChanges { .. })
}

/// Publishes every owed change the provider refuses for good on the node's
/// event bus.
pub(crate) struct PublishRefusals(pub(crate) Arc<dyn events::Bus>);

impl acp::OwedRefusalListener for PublishRefusals {
    fn refused(&self, change: &acp::OwedChange, reason: &str, kept: bool) {
        let tuple = &change.tuple;
        self.0.publish(events::Message::acp_owed_change_refused(
            events::AcpOwedChangeRefusedData {
                seq: change.seq,
                actor: change.actor.to_string(),
                policy_id: tuple.policy_id.clone(),
                resource: tuple.resource.clone(),
                object_id: tuple.object.clone(),
                relation: tuple.relation.clone(),
                subject: acp::Subject::from(tuple.subject.clone()).to_string(),
                delete: change.op == acp::OwedOp::Delete,
                reason: reason.to_owned(),
                parked: kept,
            },
        ));
    }
}

/// Settle registrations left by a previous run, then drain relationship changes.
pub(crate) async fn redrive<S>(gate: Arc<acp::ExternalDocumentAcp<S>>)
where
    S: Store + 'static,
{
    match gate.redrive_owed().await {
        Ok(report) if report.settled > 0 => tracing::info!(
            settled = report.settled,
            failed = report.failed,
            "Settled ACP registrations owed from a previous run"
        ),
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "Could not read owed ACP registrations"),
    }
    drain(gate).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_earlier_layout_stops_the_drainer_and_other_errors_do_not() {
        assert!(is_terminal(&acp::Error::LegacyOwedChanges { count: 1 }));
        assert!(!is_terminal(&acp::Error::Storage("busy".into())));
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

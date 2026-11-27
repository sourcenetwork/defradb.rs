//! Sending owed changes through a provider that journals its requests.
//!
//! The steps follow `proofs/tla/AcpOutbox.tla` with `Journaled = TRUE`:
//! a request is built for the change at the head of its tuple and the
//! changes its transaction staged right after it ([`batch`]), its digest is
//! stored with each of them (JMark), the provider's worker journals it
//! (JPrepare), its transaction hash is stored with them too (JHash), and
//! only then is it sent and its receipt waited for (JLand, JResolve).
//!
//! A change found marked is decided before anything else is done with it
//! ([`recover_marked`]; JAdopt, JDecide): applied, its record is deleted;
//! rejected, it had no effect and goes again in a request of its own;
//! unknown to the journal, the receipts and the note, it is sent again.
//! The last is where a change can take effect twice: only when its receipt
//! and the note of it can no longer be read.
//!
//! A request the provider rejects applied nothing. When it carried several
//! changes they go again one at a time, so a refusal is only ever a
//! change's own (`INV_RefusedOnItsOwn`).
//!
//! [`batch`]: ExternalDocumentAcp::batch
//! [`recover_marked`]: ExternalDocumentAcp::recover_marked

use rapidhash::{HashSetExt, RapidHashSet};
use storage::corekv::Store;

use super::settle::{object_of, Handled, ObjectKey, Step, Write};
use super::ExternalDocumentAcp;
use crate::error::Result;
use crate::outbox::{InFlight, OwedChange, OwedJournal, OwedState, RequestOutcome};

/// The most changes one request carries. The provider may carry fewer:
/// [`OwedJournal::prepare`] says how many fit.
pub const MAX_CHANGES_PER_REQUEST: usize = 16;

impl<S> ExternalDocumentAcp<S>
where
    S: Store + 'static,
{
    /// Decide `change`, found marked in flight, by what the provider knows
    /// of its request. `None` when it is to be sent again, its mark
    /// cleared; else what became of it.
    pub(super) async fn recover_marked(
        &self,
        journal: &dyn OwedJournal,
        change: &OwedChange,
    ) -> Result<Option<Step>> {
        let Some(flight) = change.in_flight.clone() else {
            return Ok(None);
        };
        let alone = match journal.outcome(&flight).await {
            RequestOutcome::Applied => {
                return Ok(Some(self.settle_applied(change).await?));
            }
            RequestOutcome::Unavailable { reason } => {
                unavailable(change, &reason);
                return Ok(Some(Step::Unavailable));
            }
            // It applied nothing. Whether it carried other changes is not
            // stored: this one goes alone, where a refusal is its own.
            RequestOutcome::Rejected { .. } => true,
            RequestOutcome::Unknown => {
                tracing::warn!(
                    target: "acp::audit",
                    seq = change.seq,
                    digest = %flight.digest,
                    tx = ?flight.tx,
                    "No journal entry, receipt or note of an owed ACP change's request; it is sent again"
                );
                false
            }
        };
        let clear = Write::ClearInFlight {
            digest: flight.digest,
            alone,
        };
        Ok(if self.write(change.seq, clear).await? {
            None
        } else {
            Some(Step::Left)
        })
    }

    /// Send `head`, the oldest change of its tuple, with the changes after
    /// it that may share its request, and store what became of each.
    pub(super) async fn send_request(
        &self,
        journal: &dyn OwedJournal,
        head: OwedChange,
        objects: Option<&RapidHashSet<ObjectKey>>,
        handled: &mut Handled,
    ) -> Result<Vec<Step>> {
        let mut changes = vec![head];
        if !changes[0].alone {
            let after = self.batch(&changes[0], objects).await?;
            changes.extend(after);
        }
        let prepared = match journal.prepare(&changes).await {
            Ok(prepared) => prepared,
            Err(answer) => return Ok(vec![self.conclude(&changes[0], answer).await?]),
        };
        changes.truncate(prepared.carries.clamp(1, changes.len()));
        let seqs: Vec<u64> = changes.iter().map(|change| change.seq).collect();

        let marked = InFlight::new(prepared.digest.clone(), None, prepared.floor)?;
        if !self.mark(&seqs, marked).await? {
            return Ok(vec![Step::Left]);
        }
        let tx = match journal.journal(&prepared).await {
            Ok(tx) => tx,
            // Whether the worker kept it is not known here: the marks stay
            // and the next pass decides by the journal or the note.
            Err(answer) => {
                tracing::debug!(
                    target: "acp::audit",
                    seq = seqs[0],
                    ?answer,
                    "An owed ACP request was not journaled; its changes stay marked"
                );
                return Ok(vec![Step::Unavailable]);
            }
        };
        let flight = InFlight::new(prepared.digest, Some(tx), prepared.floor)?;
        if !self.mark(&seqs, flight.clone()).await? {
            return Ok(vec![Step::Left]);
        }
        handled.extend(seqs.iter().skip(1));

        match journal.outcome(&flight).await {
            RequestOutcome::Applied => {
                let mut steps = Vec::with_capacity(changes.len());
                for change in &changes {
                    steps.push(self.settle_applied(change).await?);
                }
                Ok(steps)
            }
            RequestOutcome::Rejected { reason } if changes.len() == 1 => {
                let change = &changes[0];
                let answer = journal.refusal(change, reason);
                Ok(vec![self.conclude(change, answer).await?])
            }
            RequestOutcome::Rejected { reason } => {
                tracing::debug!(
                    target: "acp::audit",
                    seq = seqs[0],
                    changes = changes.len(),
                    reason,
                    "An owed ACP request was refused; its changes go again one at a time"
                );
                for change in &changes {
                    handled.remove(&change.seq);
                    let clear = Write::ClearInFlight {
                        digest: flight.digest.clone(),
                        alone: true,
                    };
                    if !self.write(change.seq, clear).await? {
                        return Ok(vec![Step::Left]);
                    }
                }
                let Some(head) = self.submittable(&changes[0]).await? else {
                    return Ok(vec![Step::Left]);
                };
                Box::pin(self.send_request(journal, head, objects, handled)).await
            }
            RequestOutcome::Unknown => {
                tracing::warn!(
                    target: "acp::audit",
                    seq = seqs[0],
                    tx = ?flight.tx,
                    "An owed ACP request left the journal without a receipt; its changes are sent again"
                );
                for change in &changes {
                    handled.remove(&change.seq);
                    let clear = Write::ClearInFlight {
                        digest: flight.digest.clone(),
                        alone: false,
                    };
                    self.write(change.seq, clear).await?;
                }
                // The node can send them now: they do not wait for a commit.
                Ok(vec![Step::Retry])
            }
            RequestOutcome::Unavailable { reason } => {
                unavailable(&changes[0], &reason);
                Ok(vec![Step::Unavailable])
            }
        }
    }

    /// The changes after `head` in the queue that may go in its request:
    /// staged by the same transaction of this run right after it, staged,
    /// unmarked and free to share a request, on an object in the pass's
    /// scope whose registration is settled, each the oldest of its tuple or
    /// behind only changes of the request.
    async fn batch(
        &self,
        head: &OwedChange,
        objects: Option<&RapidHashSet<ObjectKey>>,
    ) -> Result<Vec<OwedChange>> {
        let mut batch = Vec::new();
        if head.txn.is_none() || head.epoch.is_none() {
            return Ok(batch);
        }
        let mut members = RapidHashSet::new();
        members.insert(head.seq);
        // Only the next few can join, and the first that cannot ends the
        // batch: no more of the queue is read than a request carries.
        let candidates = self
            .next_owed_changes(head.seq, MAX_CHANGES_PER_REQUEST - 1)
            .await?;
        for change in candidates {
            let joins = change.txn == head.txn
                && change.epoch == head.epoch
                && change.state == OwedState::Staged
                && change.in_flight.is_none()
                && !change.alone
                && objects.is_none_or(|objects| objects.contains(&object_of(&change.tuple)))
                && self.joins_request(&change, &members).await?;
            if !joins {
                break;
            }
            members.insert(change.seq);
            batch.push(change);
        }
        Ok(batch)
    }

    /// Whether `change` may share a request with `members`: its object owes
    /// no registration, and every earlier unsettled change of its tuple is
    /// one of them.
    async fn joins_request(
        &self,
        change: &OwedChange,
        members: &RapidHashSet<u64>,
    ) -> Result<bool> {
        let tuple = &change.tuple;
        if self
            .owed(&tuple.policy_id, &tuple.resource, &tuple.object)
            .await?
            .is_some()
        {
            return Ok(false);
        }
        Ok(match self.oldest_on(tuple).await? {
            Some(oldest) => oldest.seq == change.seq || members.contains(&oldest.seq),
            None => false,
        })
    }

    /// Store `flight` with the changes `seqs` in one transaction.
    async fn mark(&self, seqs: &[u64], flight: InFlight) -> Result<bool> {
        let (first, others) = (seqs[0], seqs[1..].to_vec());
        self.write(first, Write::MarkInFlight { others, flight })
            .await
    }

    /// Delete the record of `change`, which took effect on the provider.
    async fn settle_applied(&self, change: &OwedChange) -> Result<Step> {
        Ok(if self.write(change.seq, Write::Settle).await? {
            let tuple = &change.tuple;
            self.object_changed(&tuple.policy_id, &tuple.resource, &tuple.object);
            Step::Settled
        } else {
            Step::Left
        })
    }
}

fn unavailable(change: &OwedChange, reason: &str) {
    tracing::debug!(
        target: "acp::audit",
        seq = change.seq,
        reason,
        "ACP provider unavailable; owed changes stay marked or staged"
    );
}

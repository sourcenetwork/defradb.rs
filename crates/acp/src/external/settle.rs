//! Submitting the relationship changes owed to an external provider.
//!
//! One pass reads the queue from its head, in sequence order, and submits
//! each staged change that is the oldest unsettled on its tuple, after the
//! registration its object still owes. What the provider answers decides the
//! change:
//!
//! - applied: its record is deleted;
//! - refused: a change whose acting DID was checked at staging is parked as
//!   refused, and one staged unchecked is deleted, so it holds no tuple and
//!   counts against no bound;
//! - a kind of change the provider cannot take at all: deleted;
//! - no usable authority: it is parked as awaiting authority;
//! - an error of the provider that does not say whether it is final: the
//!   change stays staged and the pass goes on.
//!   [`MAX_FAILED_SUBMISSIONS`] such answers in a row, whatever their
//!   words, are taken as a refusal;
//! - an error of the node's own before anything was sent: the change stays
//!   staged with the error recorded, nothing is counted, and the pass goes
//!   on. A checked change in this state holds its tuple until the node can
//!   send it; every pass says so in a warning that names it;
//! - unreachable: the pass stops, and nothing is written.
//!
//! A parked change whose acting DID was checked keeps every later change of
//! its tuple waiting behind it. Changes of other tuples go on past it. A
//! change staged unchecked never holds its tuple: when a later change of the
//! tuple comes up and the unchecked one is waiting for authority, or the
//! provider has answered it with an error, or the node could not send it,
//! the unchecked one is deleted, so it cannot be submitted after the change
//! that passed it.
//!
//! Nothing here retries or runs by itself: a pass runs after the commit of a
//! request that staged a change (over that transaction's objects), once at
//! start, before a change made outside a transaction on a tuple that has
//! changes owed, and when a caller waits for a transaction's changes, hands
//! in a fresh credential or abandons a change (see `owed.rs`). A crash
//! between the provider applying a change and its record being deleted
//! leaves the change staged, and the next pass submits it again.

use datastore::BasicTxn;
use rapidhash::{HashSetExt, RapidHashSet};
use storage::corekv::Store;
use zanzibar::types::Subject;

use super::{storage_error, ExternalDocumentAcp};
use crate::error::{Error, Result};
use crate::outbox::record::of_this_run;
use crate::outbox::{
    OwedAuthority, OwedChange, OwedState, OwedSubject, OwedTuple, StagedCount, Submission,
};
use crate::pending::PendingRegistration;

/// How many submissions in a row the provider may answer with an
/// unclassified error before the change is taken as refused. Only the
/// provider's answers count: one that could not be reached is none, and
/// neither is an error of the node's own.
pub const MAX_FAILED_SUBMISSIONS: u32 = 5;

/// Which owed changes a pass submits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwedScope {
    /// Everything owed.
    All,
    /// What is owed on the objects the transaction with this ID staged
    /// changes on: its own changes and those that precede them there.
    Transaction(u64),
}

/// What one pass over the owed relationship changes did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwedSettlement {
    /// Changes the provider holds and whose records were deleted.
    pub settled: u64,
    /// Changes the provider refused for good, now parked as refused.
    pub refused: u64,
    /// Changes deleted without the provider holding them: staged unchecked
    /// and refused, or passed by a later change of their tuple; or of a kind
    /// the provider cannot take.
    pub discarded: u64,
    /// Changes whose recorded authority no longer lets the node submit them.
    pub awaiting_authority: u64,
    /// Changes the provider answered with an unclassified error, still
    /// staged.
    pub failed: u64,
    /// Staged changes the pass came to and did not submit: behind an earlier
    /// change of their tuple, behind their object's owed registration,
    /// changed by a transaction that committed meanwhile, or not sent for an
    /// error of the node's own. One of the last kind that a later change
    /// then passes is counted in `discarded` too.
    pub left: u64,
    /// The provider could not be reached and the pass stopped there; what
    /// follows in the queue was not looked at.
    pub provider_unavailable: bool,
}

/// What became of one change in a pass.
enum Step {
    Settled,
    Refused,
    Discarded,
    Awaiting,
    Failed,
    Left,
    /// Already refused or awaiting authority, or outside the pass's scope:
    /// not submitted.
    Skipped,
    Unavailable,
}

/// What the registration an object owes came to.
enum Registration {
    /// Nothing is owed for the object any more.
    Done,
    /// It stays owed, and so do the object's changes.
    Held,
    Unavailable,
}

/// Why a change a later one passed was deleted.
const PASSED: &str = "a later change of its relationship passed it";
/// Why an abandoned change was deleted.
const ABANDONED: &str = "its actor abandoned it";

/// A write to one owed change's record.
pub(super) enum Write {
    Settle,
    Mark(OwedState, u32, String),
    /// Record a fresh authority and make the change owed again.
    Reauthorize(OwedAuthority),
    /// Delete the record if the change is still refused, and keep a note
    /// that its actor gave it up.
    Abandon,
    /// Delete the record without the provider holding the change, and keep
    /// a note of why.
    Discard(String),
}

pub(super) type ObjectKey = (String, String, String);

pub(super) fn object_of(tuple: &OwedTuple) -> ObjectKey {
    (
        tuple.policy_id.clone(),
        tuple.resource.clone(),
        tuple.object.clone(),
    )
}

impl<S> ExternalDocumentAcp<S>
where
    S: Store + 'static,
{
    /// Submit the owed relationship changes `scope` names once, from the
    /// head of the queue. One pass runs at a time; a second waits for the
    /// first.
    pub async fn settle_owed_changes_once(&self, scope: OwedScope) -> Result<OwedSettlement> {
        let _pass = self.settling.lock().await;
        let objects = match scope {
            OwedScope::All => None,
            OwedScope::Transaction(txn) => {
                let objects = self.objects_staged_by(txn).await?;
                if objects.is_empty() {
                    return Ok(OwedSettlement::default());
                }
                Some(objects)
            }
        };
        self.pass(objects.as_ref()).await
    }

    /// The objects the changes transaction `txn` staged are on.
    async fn objects_staged_by(&self, txn: u64) -> Result<RapidHashSet<ObjectKey>> {
        let mut objects = RapidHashSet::new();
        let mut after = None;
        loop {
            let page = self.owed_changes_after(after).await?;
            let Some(last) = page.last() else {
                return Ok(objects);
            };
            after = Some(last.seq);
            objects.extend(
                page.iter()
                    .filter(|change| of_this_run(change.txn, change.epoch, txn))
                    .map(|change| object_of(&change.tuple)),
            );
        }
    }

    /// One pass over the changes on `objects`, or over all of them. The
    /// caller holds the pass lock.
    pub(super) async fn pass(
        &self,
        objects: Option<&RapidHashSet<ObjectKey>>,
    ) -> Result<OwedSettlement> {
        let mut report = OwedSettlement::default();
        let mut held = RapidHashSet::new();
        let mut after = None;
        loop {
            let page = self.owed_changes_after(after).await?;
            let Some(last) = page.last() else {
                break;
            };
            after = Some(last.seq);
            for change in &page {
                if objects.is_some_and(|objects| !objects.contains(&object_of(&change.tuple))) {
                    continue;
                }
                report.discarded += self.pass_unchecked(change).await?;
                match self.settle_change(change, &mut held).await? {
                    Step::Settled => report.settled += 1,
                    Step::Refused => report.refused += 1,
                    Step::Discarded => report.discarded += 1,
                    Step::Awaiting => report.awaiting_authority += 1,
                    Step::Failed => report.failed += 1,
                    Step::Left => report.left += 1,
                    Step::Skipped => {}
                    Step::Unavailable => {
                        report.provider_unavailable = true;
                        return Ok(report);
                    }
                }
            }
        }
        if report.refused > 0 || report.awaiting_authority > 0 || report.discarded > 0 {
            tracing::warn!(
                target: "acp::audit",
                refused = report.refused,
                awaiting_authority = report.awaiting_authority,
                discarded = report.discarded,
                "Owed ACP changes were parked or discarded; later changes of a parked one's tuple wait behind it"
            );
        }
        Ok(report)
    }

    /// Run a pass before a relationship change made outside a transaction
    /// goes to the provider, when anything is owed on its tuple: the direct
    /// change is the later one, and must not reach the provider before a
    /// staged one it follows. While a staged change stays owed on the tuple
    /// after the pass (the provider was not reached, or the object's
    /// registration is still owed), the direct change is refused with
    /// [`Error::OwedChangesPending`]. A parked change is overtaken: nothing
    /// submits it again by itself.
    pub(super) async fn settle_owed_before(
        &self,
        subject: &Subject,
        policy_id: &str,
        resource: &str,
        object: &str,
        relation: &str,
    ) -> Result<()> {
        let Ok(subject) = OwedSubject::try_from(subject.clone()) else {
            return Ok(());
        };
        let subject = match subject {
            OwedSubject::Actor { did } => OwedSubject::Actor {
                did: self.provider.canonical_actor(&did),
            },
            other => other,
        };
        let tuple = OwedTuple {
            policy_id: policy_id.to_owned(),
            resource: resource.to_owned(),
            object: object.to_owned(),
            relation: relation.to_owned(),
            subject,
        };
        if self.oldest_on(&tuple).await?.is_none() {
            return Ok(());
        }
        {
            let _pass = self.settling.lock().await;
            let objects = RapidHashSet::from_iter([object_of(&tuple)]);
            self.pass(Some(&objects)).await?;
        }
        match self.oldest_on(&tuple).await? {
            Some(oldest) if oldest.state == OwedState::Staged => Err(Error::OwedChangesPending {
                object: object.to_owned(),
                relation: relation.to_owned(),
            }),
            _ => Ok(()),
        }
    }

    /// The oldest unsettled change of `tuple`, as committed now.
    pub(super) async fn oldest_on(&self, tuple: &OwedTuple) -> Result<Option<OwedChange>> {
        let txn = self.begin(true).await?;
        let changes = self.owed_changes(&txn.acpstore(), StagedCount::new());
        let oldest = match changes.tuple(tuple).await {
            Ok(Some(state)) => changes.get(state.oldest).await,
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        };
        drop(changes);
        txn.discard()
            .map_err(|error| Error::Storage(error.to_string()))?;
        oldest
    }

    pub(super) async fn begin(&self, readonly: bool) -> Result<BasicTxn> {
        BasicTxn::new(&**self.store.inner(), 0, readonly)
            .await
            .map_err(storage_error)
    }

    pub(super) async fn owed_changes_after(&self, after: Option<u64>) -> Result<Vec<OwedChange>> {
        let txn = self.begin(true).await?;
        let changes = self.owed_changes(&txn.acpstore(), StagedCount::new());
        let page = changes.page(after).await;
        drop(changes);
        txn.discard()
            .map_err(|error| Error::Storage(error.to_string()))?;
        page
    }

    /// Delete the unchecked changes `change` passes: earlier changes of its
    /// tuple, staged without the authority check, that the provider has not
    /// taken (they wait for authority, it answered them with an error, or
    /// the node could not send them).
    /// Nobody vouched for them, so they may not keep a later change from the
    /// provider, and once passed they must never be submitted after it.
    /// Returns how many were deleted.
    async fn pass_unchecked(&self, change: &OwedChange) -> Result<u64> {
        if change.state != OwedState::Staged {
            return Ok(0);
        }
        let mut passed = 0;
        while let Some(oldest) = self.oldest_on(&change.tuple).await? {
            let yields = oldest.seq < change.seq
                && !oldest.authority_checked
                && match oldest.state {
                    OwedState::AwaitingAuthority => true,
                    // Tried, and not taken: answered with an error, or
                    // never sent.
                    OwedState::Staged => oldest.last_error.is_some(),
                    OwedState::Refused { .. } => false,
                };
            if !yields
                || !self
                    .write(oldest.seq, Write::Discard(PASSED.to_owned()))
                    .await?
            {
                break;
            }
            discarded(&oldest, PASSED);
            passed += 1;
        }
        Ok(passed)
    }

    /// `change` as committed now, when it is staged and the oldest unsettled
    /// change of its tuple.
    async fn submittable(&self, change: &OwedChange) -> Result<Option<OwedChange>> {
        let txn = self.begin(true).await?;
        let changes = self.owed_changes(&txn.acpstore(), StagedCount::new());
        let current = changes.get(change.seq).await;
        let tuple = changes.tuple(&change.tuple).await;
        drop(changes);
        txn.discard()
            .map_err(|error| Error::Storage(error.to_string()))?;
        let head = tuple?.is_some_and(|state| state.oldest == change.seq);
        Ok(current?.filter(|current| head && current.state == OwedState::Staged))
    }

    async fn settle_change(
        &self,
        change: &OwedChange,
        held: &mut RapidHashSet<ObjectKey>,
    ) -> Result<Step> {
        if change.state != OwedState::Staged {
            return Ok(Step::Skipped);
        }
        let Some(change) = self.submittable(change).await? else {
            return Ok(Step::Left);
        };
        let object = object_of(&change.tuple);
        if held.contains(&object) {
            return Ok(Step::Left);
        }
        match self.register_first(&change).await? {
            Registration::Done => {}
            Registration::Held => {
                held.insert(object);
                return Ok(Step::Left);
            }
            Registration::Unavailable => return Ok(Step::Unavailable),
        }
        let attempts = change.attempts.saturating_add(1);
        let (write, step) = match self.provider.submit_owed_change(&change).await {
            Submission::Applied => (Write::Settle, Step::Settled),
            Submission::Refused { reason } => refusal(&change, attempts, reason),
            Submission::AwaitingAuthority { reason } => (
                Write::Mark(OwedState::AwaitingAuthority, attempts, reason),
                Step::Awaiting,
            ),
            Submission::Failed { reason } if attempts >= MAX_FAILED_SUBMISSIONS => {
                refusal(&change, attempts, reason)
            }
            Submission::Failed { reason } => (
                Write::Mark(OwedState::Staged, attempts, reason),
                Step::Failed,
            ),
            Submission::Unsupported { reason } => {
                discarded(&change, &reason);
                (Write::Discard(reason), Step::Discarded)
            }
            // The node's own error: kept with the record so the change is
            // seen not to have gone, and never counted against it.
            Submission::NotSent { reason } => {
                tracing::warn!(
                    target: "acp::audit",
                    seq = change.seq,
                    op = ?change.op,
                    actor = %change.actor,
                    object = %change.tuple.object,
                    relation = %change.tuple.relation,
                    authority_checked = change.authority_checked,
                    reason,
                    "Could not send an owed ACP change; it stays staged and holds its tuple unless it was staged unchecked"
                );
                if change.last_error.as_deref() != Some(reason.as_str()) {
                    self.write(
                        change.seq,
                        Write::Mark(OwedState::Staged, change.attempts, reason),
                    )
                    .await?;
                }
                return Ok(Step::Left);
            }
            Submission::Unavailable { reason } => {
                tracing::debug!(
                    target: "acp::audit",
                    seq = change.seq,
                    reason,
                    "ACP provider unavailable; owed changes stay staged"
                );
                return Ok(Step::Unavailable);
            }
        };
        Ok(if self.write(change.seq, write).await? {
            step
        } else {
            Step::Left
        })
    }

    /// The delegation that lets the node send the registration `owed` when
    /// its own record no longer does: the first, in queue order, of those
    /// recorded with the changes its creator still owes on the same object
    /// that admits the registration. `None` when the record serves, or
    /// nothing does. A pass and the report both decide by this, so what is
    /// reported as sendable is what a pass sends.
    pub(super) async fn creators_delegation(
        &self,
        owed: &PendingRegistration,
    ) -> Result<Option<String>> {
        if self.recorded_registrar(owed, None).is_ok() {
            return Ok(None);
        }
        let creator = self.provider.canonical_actor(&owed.creator);
        let mut after = None;
        loop {
            let page = self.owed_changes_after(after).await?;
            let Some(last) = page.last() else {
                return Ok(None);
            };
            after = Some(last.seq);
            for change in page {
                let on_object = change.tuple.policy_id == owed.policy_id
                    && change.tuple.resource == owed.resource_name
                    && change.tuple.object == owed.doc_id;
                if let (true, OwedAuthority::Delegation { token }) =
                    (on_object && change.actor == creator, change.authority)
                {
                    if self.recorded_registrar(owed, Some(&token)).is_ok() {
                        return Ok(Some(token));
                    }
                }
            }
        }
    }

    /// Settle the registration `change`'s object still owes, if any, with
    /// what its record holds, or with a delegation recorded with a change
    /// its creator owes on the object ([`Self::creators_delegation`]).
    async fn register_first(&self, change: &OwedChange) -> Result<Registration> {
        let tuple = &change.tuple;
        let Some(owed) = self
            .owed(&tuple.policy_id, &tuple.resource, &tuple.object)
            .await?
        else {
            return Ok(Registration::Done);
        };
        let also = self.creators_delegation(&owed).await?;
        match self.settle_with(&owed, also.as_deref()).await {
            Ok(()) => {}
            Err(Error::ProviderUnavailable(reason)) => {
                tracing::debug!(
                    target: "acp::audit",
                    doc_id = %owed.doc_id,
                    reason,
                    "ACP provider unavailable; owed changes stay staged"
                );
                return Ok(Registration::Unavailable);
            }
            Err(error) => {
                tracing::debug!(
                    target: "acp::audit",
                    doc_id = %owed.doc_id,
                    %error,
                    "Owed ACP registration not settled; its document's changes wait"
                );
                return Ok(Registration::Held);
            }
        }
        // A record written again meanwhile is another registration to settle.
        Ok(
            match self
                .owed(&tuple.policy_id, &tuple.resource, &tuple.object)
                .await?
            {
                Some(_) => Registration::Held,
                None => Registration::Done,
            },
        )
    }

    /// Write to the record of change `seq` in a transaction of its own, its
    /// scans validated. `false` when the change is gone or a transaction that
    /// committed meanwhile won: the next pass finds the change as it is then.
    pub(super) async fn write(&self, seq: u64, write: Write) -> Result<bool> {
        let txn = self.begin(false).await?;
        let view = txn.acpstore();
        view.set_scans_validated(true);
        let changes = self.owed_changes(&view, StagedCount::new());
        let written = match write {
            Write::Settle => changes.settle(seq).await,
            Write::Mark(state, attempts, reason) => {
                changes.mark(seq, state, attempts, Some(reason)).await
            }
            Write::Reauthorize(authority) => changes.reauthorize(seq, authority).await,
            Write::Discard(reason) => changes.discard(seq, &reason).await,
            Write::Abandon => match changes.get(seq).await {
                Ok(Some(change)) if matches!(change.state, OwedState::Refused { .. }) => {
                    changes.discard(seq, ABANDONED).await
                }
                Ok(_) => Ok(false),
                Err(error) => Err(error),
            },
        };
        drop(changes);
        drop(view);
        match written {
            Ok(true) => match txn.commit().await {
                Ok(()) => {
                    self.changed.notify_waiters();
                    Ok(true)
                }
                Err(datastore::Error::Storage(error)) if error.is_txn_conflict() => Ok(false),
                Err(error) => Err(Error::Storage(error.to_string())),
            },
            other => {
                txn.discard()
                    .map_err(|error| Error::Storage(error.to_string()))?;
                other
            }
        }
    }
}

/// What a refusal does to `change`: one whose acting DID was checked at
/// staging is parked, and one staged unchecked is deleted. Nobody vouched
/// for the unchecked one and the provider said no, so it may not hold its
/// tuple or count against a bound.
fn refusal(change: &OwedChange, attempts: u32, reason: String) -> (Write, Step) {
    if change.authority_checked {
        (
            Write::Mark(
                OwedState::Refused {
                    reason: reason.clone(),
                },
                attempts,
                reason,
            ),
            Step::Refused,
        )
    } else {
        discarded(change, &reason);
        (Write::Discard(reason), Step::Discarded)
    }
}

/// Say which change was deleted without the provider holding it, and why.
fn discarded(change: &OwedChange, reason: &str) {
    tracing::warn!(
        target: "acp::audit",
        seq = change.seq,
        op = ?change.op,
        actor = %change.actor,
        object = %change.tuple.object,
        relation = %change.tuple.relation,
        authority_checked = change.authority_checked,
        reason,
        "Owed ACP change discarded"
    );
}

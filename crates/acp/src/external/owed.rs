//! Reporting what is owed to an external provider, waiting for it, handing
//! in a fresh credential for it, and abandoning a refused change.
//!
//! A fresh credential arrives as the credentials of the request this gate
//! serves ([`crate::DocumentACP::bind_request`]). It is asked about exactly
//! the owed command, with the question a create or a staged change is asked
//! when it is admitted ([`crate::DocumentACP::command_authority`]), recorded
//! with the entry, and the entry is submitted. A credential the question
//! refuses records nothing.
//!
//! Only a change the provider refused can be abandoned. The provider
//! answered it, so no request for it is still on the way. A change that
//! waits for authority may have an earlier request in flight (a commit that
//! stopped waiting leaves one), and deleting its record would not stop that
//! request. An abandoned delete stops denying its subject: the provider,
//! which refused the delete, decides again.

use identity::Did;
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};
use storage::corekv::Store;

use super::settle::{object_of, ObjectKey, Write};
use super::{ExternalDocumentAcp, OwedScope};
use crate::error::{Error, Result};
use crate::outbox::record::of_this_run;
use crate::outbox::{
    OwedAuthority, OwedChange, OwedEntry, OwedFilter, OwedHold, OwedOutcome, OwedRef, OwedState,
    OwedWait, OwedWhat, StagedCount,
};
use crate::pending::{PendingRegistration, PENDING_PREFIX};
use crate::provider_command::ProviderCommand;

fn state_name(state: &OwedState) -> &'static str {
    match state {
        OwedState::Staged => "staged",
        OwedState::Refused { .. } => "refused",
        OwedState::AwaitingAuthority => "awaiting authority",
    }
}

fn registration_entry(owed: PendingRegistration, sendable: bool) -> OwedEntry {
    OwedEntry {
        what: OwedWhat::Registration,
        actor: owed.creator,
        policy_id: owed.policy_id,
        resource: owed.resource_name,
        object: owed.doc_id,
        txn: owed.txn,
        sendable,
        state: owed.state,
        reason: owed.last_error,
        attempts: owed.attempts,
        held_by: None,
        authority: owed.authority.as_ref().map(Into::into),
        discarded: false,
    }
}

/// The note of a change whose record was deleted without the provider
/// holding it, as an entry.
fn discarded_entry(note: OwedChange) -> OwedEntry {
    OwedEntry {
        what: OwedWhat::Change {
            seq: note.seq,
            op: note.op,
            relation: note.tuple.relation,
            subject: note.tuple.subject,
        },
        actor: note.actor,
        policy_id: note.tuple.policy_id,
        resource: note.tuple.resource,
        object: note.tuple.object,
        txn: note.txn,
        state: note.state,
        reason: note.last_error,
        attempts: note.attempts,
        held_by: None,
        authority: None,
        sendable: false,
        discarded: true,
    }
}

/// What a write to the record of a change that did not go through means:
/// the change is gone, or a transaction that committed meanwhile won.
fn unwritten(entry: &OwedRef, actor: &Did, still_owed: bool) -> Error {
    if still_owed {
        Error::OwedEntryConflict {
            entry: entry.to_string(),
        }
    } else {
        Error::OwedEntryNotFound {
            entry: entry.to_string(),
            actor: actor.to_string(),
        }
    }
}

impl<S> ExternalDocumentAcp<S>
where
    S: Store + 'static,
{
    fn not_found(entry: &OwedRef, actor: &Did) -> Error {
        unwritten(entry, actor, false)
    }

    fn same_actor(&self, left: &Did, right: &Did) -> bool {
        self.provider.canonical_actor(left) == self.provider.canonical_actor(right)
    }

    /// What is owed to the provider that `filter` names: registrations
    /// first, then changes in queue order, then the notes of changes whose
    /// records were deleted without the provider holding them. Every entry
    /// of one actor or one transaction is found by reading all that is
    /// owed, a page at a time.
    pub async fn entries(&self, filter: OwedFilter<'_>) -> Result<Vec<OwedEntry>> {
        let mut entries = Vec::new();
        let mut registrations = RapidHashMap::new();
        let mut owed = Vec::new();
        match filter {
            OwedFilter::Entry(OwedRef::Change { .. }) => {}
            OwedFilter::Entry(OwedRef::Registration {
                policy_id,
                resource,
                object,
            }) => owed.extend(self.owed(policy_id, resource, object).await?),
            OwedFilter::Actor(_) | OwedFilter::Transaction(_) => {
                let mut start = PENDING_PREFIX.to_vec();
                loop {
                    let page = self.owed_page(&start).await?;
                    let Some(last) = page.last() else {
                        break;
                    };
                    start = last.key();
                    start.push(0);
                    owed.extend(page.into_iter().filter(|owed| match filter {
                        OwedFilter::Actor(actor) => self.same_actor(&owed.creator, actor),
                        OwedFilter::Transaction(txn) => of_this_run(owed.txn, owed.epoch, txn),
                        OwedFilter::Entry(_) => false,
                    }));
                }
            }
        }
        for owed in owed {
            let sendable = self.registration_sendable(&owed).await?;
            entries.push(registration_entry(owed, sendable));
        }
        let wanted = |change: &OwedChange| match filter {
            OwedFilter::Actor(actor) => change.actor == self.provider.canonical_actor(actor),
            OwedFilter::Transaction(txn) => of_this_run(change.txn, change.epoch, txn),
            OwedFilter::Entry(OwedRef::Change { seq }) => change.seq == *seq,
            OwedFilter::Entry(OwedRef::Registration { .. }) => false,
        };
        let mut after = match filter {
            OwedFilter::Entry(OwedRef::Registration { .. }) => return Ok(entries),
            OwedFilter::Entry(OwedRef::Change { seq }) => seq.checked_sub(1),
            OwedFilter::Actor(_) | OwedFilter::Transaction(_) => None,
        };
        loop {
            let page = self.owed_changes_after(after).await?;
            let Some(last) = page.last() else {
                break;
            };
            after = Some(last.seq);
            for change in page.into_iter().filter(|change| wanted(change)) {
                entries.push(self.change_entry(change, &mut registrations).await?);
            }
            if matches!(filter, OwedFilter::Entry(_)) {
                break;
            }
        }
        entries.extend(
            self.discard_notes(match filter {
                OwedFilter::Entry(OwedRef::Change { seq }) => Some(*seq),
                _ => None,
            })
            .await?
            .into_iter()
            .filter(|note| wanted(note))
            .map(discarded_entry),
        );
        Ok(entries)
    }

    /// Whether a submission could still move the registration `owed`: it is
    /// staged, or it waits for authority the node now has, by its own
    /// record or by the delegation of a change its creator owes on the same
    /// object. One the provider refused is not sent again before the next
    /// start.
    async fn registration_sendable(&self, owed: &PendingRegistration) -> Result<bool> {
        match owed.state {
            OwedState::Staged => return Ok(true),
            OwedState::Refused { .. } => return Ok(false),
            OwedState::AwaitingAuthority => {}
        }
        if self.recorded_registrar(owed, None).is_ok() {
            return Ok(true);
        }
        let creator = self.provider.canonical_actor(&owed.creator);
        let mut after = None;
        loop {
            let page = self.owed_changes_after(after).await?;
            let Some(last) = page.last() else {
                return Ok(false);
            };
            after = Some(last.seq);
            for change in &page {
                let on_object = change.tuple.policy_id == owed.policy_id
                    && change.tuple.resource == owed.resource_name
                    && change.tuple.object == owed.doc_id;
                if let (true, OwedAuthority::Delegation { token }) =
                    (on_object && change.actor == creator, &change.authority)
                {
                    if self.recorded_registrar(owed, Some(token)).is_ok() {
                        return Ok(true);
                    }
                }
            }
        }
    }

    /// The notes of discarded changes: the one of `seq`, or all of them.
    async fn discard_notes(&self, seq: Option<u64>) -> Result<Vec<OwedChange>> {
        let txn = self.begin(true).await?;
        let changes = self.owed_changes(&txn.acpstore(), StagedCount::new());
        let notes = match seq {
            Some(seq) => changes
                .discarded(seq)
                .await
                .map(|note| note.into_iter().collect()),
            None => changes.discards().await,
        };
        drop(changes);
        txn.discard()
            .map_err(|error| Error::Storage(error.to_string()))?;
        notes
    }

    /// Delete the notes of the discarded changes among `entries`: whoever
    /// is given these entries has been told.
    async fn acknowledge(&self, entries: &[OwedEntry]) -> Result<()> {
        let told: Vec<u64> = entries
            .iter()
            .filter(|entry| entry.discarded)
            .filter_map(|entry| match entry.what {
                OwedWhat::Change { seq, .. } => Some(seq),
                OwedWhat::Registration => None,
            })
            .collect();
        if told.is_empty() {
            return Ok(());
        }
        let txn = self.begin(false).await?;
        let changes = self.owed_changes(&txn.acpstore(), StagedCount::new());
        let mut deleted = Ok(());
        for seq in told {
            deleted = changes.acknowledge(seq).await;
            if deleted.is_err() {
                break;
            }
        }
        drop(changes);
        match deleted {
            Ok(()) => match txn.commit().await {
                Ok(()) => Ok(()),
                // A note another caller deleted first was told to that one.
                Err(datastore::Error::Storage(error)) if error.is_txn_conflict() => Ok(()),
                Err(error) => Err(Error::Storage(error.to_string())),
            },
            Err(error) => {
                txn.discard()
                    .map_err(|error| Error::Storage(error.to_string()))?;
                Err(error)
            }
        }
    }

    /// `change` as it is reported: with what it waits behind, and whether
    /// the node can still send it. `registrations` remembers what each
    /// object owes and whether that can be sent.
    async fn change_entry(
        &self,
        change: OwedChange,
        registrations: &mut RapidHashMap<ObjectKey, Option<bool>>,
    ) -> Result<OwedEntry> {
        let tuple = &change.tuple;
        let object = object_of(tuple);
        if !registrations.contains_key(&object) {
            let sendable = match self
                .owed(&tuple.policy_id, &tuple.resource, &tuple.object)
                .await?
            {
                Some(owed) => Some(self.registration_sendable(&owed).await?),
                None => None,
            };
            registrations.insert(object.clone(), sendable);
        }
        let staged = change.state == OwedState::Staged;
        let (held_by, sendable) = match registrations.get(&object).copied().flatten() {
            Some(registration) => (Some(OwedHold::Registration), staged && registration),
            None => match self.oldest_on(tuple).await? {
                // An unchecked change that is parked holds nothing: a later
                // change of its tuple passes it.
                Some(oldest)
                    if oldest.seq != change.seq
                        && (oldest.authority_checked || oldest.state == OwedState::Staged) =>
                {
                    (
                        Some(OwedHold::Change {
                            seq: (oldest.actor == change.actor).then_some(oldest.seq),
                        }),
                        staged && oldest.state == OwedState::Staged,
                    )
                }
                _ => (None, staged),
            },
        };
        Ok(OwedEntry {
            what: OwedWhat::Change {
                seq: change.seq,
                op: change.op,
                relation: change.tuple.relation,
                subject: change.tuple.subject,
            },
            actor: change.actor,
            policy_id: change.tuple.policy_id,
            resource: change.tuple.resource,
            object: change.tuple.object,
            txn: change.txn,
            state: change.state,
            reason: change.last_error,
            attempts: change.attempts,
            held_by,
            authority: Some((&change.authority).into()),
            sendable,
            discarded: false,
        })
    }

    /// What transaction `txn` of this run still owes, as `only` sees it:
    /// everything, or the entries of one actor.
    async fn of_transaction(&self, txn: u64, only: Option<&Did>) -> Result<Vec<OwedEntry>> {
        let mut entries = self.entries(OwedFilter::Transaction(txn)).await?;
        if let Some(actor) = only {
            entries.retain(|entry| self.same_actor(&entry.actor, actor));
        }
        Ok(entries)
    }

    /// Wait until what transaction `txn` of this run owes the provider is
    /// settled or has come to rest, and say which, for the entries of
    /// `only` or for all of them. Another actor's entries neither show nor
    /// keep the wait. The first look submits what the node can still send,
    /// once; after that the wait ends when a record changes so that nothing
    /// is left to send. It returns early, with what is still pending, when
    /// the provider could not be reached: nothing on this node sends again
    /// by itself. The caller bounds the wait.
    ///
    /// Every record written wakes every wait, which then reads all that is
    /// owed again.
    pub async fn await_transaction(&self, txn: u64, only: Option<&Did>) -> Result<OwedWait> {
        let mut submitted = false;
        loop {
            // Asked for before the read, so a record that changes after it
            // is not missed.
            let changed = self.changed.notified();
            let entries = self.of_transaction(txn, only).await?;
            if !entries.iter().any(|entry| entry.sendable) {
                self.acknowledge(&entries).await?;
                return Ok(OwedWait::of(entries, false));
            }
            if submitted {
                changed.await;
                continue;
            }
            submitted = true;
            if self.submit_transaction(txn, &entries).await? {
                let entries = self.of_transaction(txn, only).await?;
                self.acknowledge(&entries).await?;
                return Ok(OwedWait::of(entries, true));
            }
        }
    }

    /// Submit once what transaction `txn` owes: its registrations, then the
    /// changes on the objects it staged on. `true` when the provider could
    /// not be reached.
    async fn submit_transaction(&self, txn: u64, entries: &[OwedEntry]) -> Result<bool> {
        for entry in entries {
            if entry.what != OwedWhat::Registration {
                continue;
            }
            match self
                .settle_if_owed(&entry.policy_id, &entry.resource, &entry.object)
                .await
            {
                Ok(()) => {}
                Err(Error::ProviderUnavailable(_)) => return Ok(true),
                // What it came to is in its record.
                Err(_) => {}
            }
        }
        Ok(self
            .settle_owed_changes_once(OwedScope::Transaction(txn))
            .await?
            .provider_unavailable)
    }

    /// Take the credential of the request this gate serves as the fresh
    /// authority of one entry `actor` owes, and submit the entry.
    ///
    /// The credential is asked about exactly the owed command, as at
    /// admission; refused, the call fails with
    /// [`Error::OwedAuthorityUnavailable`] and the entry stays as it was.
    /// Admitted, it is recorded with the entry, which is owed again, and
    /// what is owed on the entry's object is submitted once, in order: a
    /// registration that settles lets the changes behind it go, each with
    /// its own authority. An entry of another actor is not found.
    pub async fn reauthorize(&self, actor: &Did, entry: &OwedRef) -> Result<OwedOutcome> {
        // The credential is the one the request itself carries for the
        // actor. A request that carries none hands in nothing but a key the
        // node may hold: it is not asked as it is, or the provider would
        // take the token the process holds for that DID.
        let asked = if self.carries_bearer_for(actor) {
            std::sync::Arc::clone(&self.provider)
        } else {
            self.presenting(actor, None, false)
        };
        let object = match entry {
            OwedRef::Registration {
                policy_id,
                resource,
                object,
            } => {
                let owed = self
                    .owed(policy_id, resource, object)
                    .await?
                    .filter(|owed| self.same_actor(&owed.creator, actor))
                    .ok_or_else(|| Self::not_found(entry, actor))?;
                let authority = asked.command_authority(
                    actor,
                    ProviderCommand::RegisterObject {
                        policy_id,
                        resource,
                        object,
                    },
                )?;
                // Kept as it was presented: only one of the two ways of
                // presenting a delegation admits it.
                let request_bound = match &authority {
                    OwedAuthority::Delegation { token } => self
                        .presenting(&owed.creator, Some(token), true)
                        .command_authority(
                            &owed.creator,
                            ProviderCommand::RegisterObject {
                                policy_id,
                                resource,
                                object,
                            },
                        )
                        .is_ok(),
                    OwedAuthority::NodeKey => false,
                };
                let owed = self
                    .rewrite(&owed, |record| {
                        record.authority = Some(authority);
                        record.request_bound = request_bound;
                        record.state = OwedState::Staged;
                        record.attempts = 0;
                        record.last_error = None;
                    })
                    .await?
                    .ok_or_else(|| Self::not_found(entry, actor))?;
                // What the submission came to is in the record.
                let _ = self.settle(&owed).await;
                (policy_id.clone(), resource.clone(), object.clone())
            }
            OwedRef::Change { seq } => {
                let change = self
                    .change(*seq)
                    .await?
                    .filter(|change| change.actor == self.provider.canonical_actor(actor))
                    .ok_or_else(|| Self::not_found(entry, actor))?;
                let authority = asked.command_authority(
                    actor,
                    ProviderCommand::Relationship {
                        tuple: &change.tuple,
                        op: change.op,
                    },
                )?;
                let _pass = self.settling.lock().await;
                if !self.write(*seq, Write::Reauthorize(authority)).await? {
                    let still_owed = self.change(*seq).await?.is_some();
                    return Err(unwritten(entry, actor, still_owed));
                }
                object_of(&change.tuple)
            }
        };
        self.submit_object(object).await?;
        Ok(match self.entries(OwedFilter::Entry(entry)).await?.pop() {
            Some(left) if left.discarded => {
                self.acknowledge(std::slice::from_ref(&left)).await?;
                OwedOutcome::Discarded(Box::new(left))
            }
            Some(left) => OwedOutcome::Owed(Box::new(left)),
            None => OwedOutcome::Settled,
        })
    }

    /// Delete the record of the refused change at `seq`, which `actor`
    /// staged, without the provider holding it. The change stops holding
    /// the later changes of its tuple, which are submitted once, and a
    /// delete stops denying its subject. A change in any other state is
    /// [`Error::OwedChangeNotRefused`]; one of another actor is not found;
    /// and when a transaction that committed meanwhile changed what is owed
    /// on the relationship, nothing is written and the call fails with
    /// [`Error::OwedEntryConflict`], to be made again.
    ///
    /// Once the record is deleted the call has succeeded. Whatever the
    /// submission of the changes behind it comes to is in their records.
    pub async fn abandon_change(&self, actor: &Did, seq: u64) -> Result<()> {
        let entry = OwedRef::Change { seq };
        let change = {
            let _pass = self.settling.lock().await;
            let change = self
                .change(seq)
                .await?
                .filter(|change| change.actor == self.provider.canonical_actor(actor))
                .ok_or_else(|| Self::not_found(&entry, actor))?;
            if !matches!(change.state, OwedState::Refused { .. }) {
                return Err(Error::OwedChangeNotRefused {
                    seq,
                    state: state_name(&change.state),
                });
            }
            if !self.write(seq, Write::Abandon).await? {
                let still_owed = self.change(seq).await?.is_some();
                return Err(unwritten(&entry, actor, still_owed));
            }
            change
        };
        tracing::warn!(
            target: "acp::audit",
            seq,
            op = ?change.op,
            actor = %change.actor,
            object = %change.tuple.object,
            relation = %change.tuple.relation,
            "Owed ACP change abandoned by its actor; the provider does not hold it"
        );
        let behind = match self.oldest_on(&change.tuple).await {
            Ok(Some(_)) => self.submit_object(object_of(&change.tuple)).await,
            Ok(None) => Ok(()),
            Err(error) => Err(error),
        };
        if let Err(error) = behind {
            tracing::warn!(
                target: "acp::audit",
                seq,
                %error,
                "Could not submit the changes behind an abandoned ACP change; they stay owed"
            );
        }
        Ok(())
    }

    /// The owed change at `seq`, as committed now.
    async fn change(&self, seq: u64) -> Result<Option<OwedChange>> {
        let txn = self.begin(true).await?;
        let changes = self.owed_changes(&txn.acpstore(), StagedCount::new());
        let change = changes.get(seq).await;
        drop(changes);
        txn.discard()
            .map_err(|error| Error::Storage(error.to_string()))?;
        change
    }

    /// Submit once what is owed on one object, in order.
    async fn submit_object(&self, object: ObjectKey) -> Result<()> {
        let _pass = self.settling.lock().await;
        let mut objects = RapidHashSet::new();
        objects.insert(object);
        self.pass(Some(&objects)).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_that_lost_to_a_commit_is_a_conflict_and_not_a_missing_entry() {
        let actor = Did::new("did:key:actor").unwrap();
        let entry = OwedRef::Change { seq: 7 };
        assert!(matches!(
            unwritten(&entry, &actor, true),
            Error::OwedEntryConflict { entry } if entry == "change 7"
        ));
        assert!(matches!(
            unwritten(&entry, &actor, false),
            Error::OwedEntryNotFound { .. }
        ));
    }
}

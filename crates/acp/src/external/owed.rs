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
use crate::outbox::{
    OwedAuthority, OwedChange, OwedEntry, OwedFilter, OwedOutcome, OwedRef, OwedState, OwedWait,
    OwedWhat, StagedCount,
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

fn registration_ref(owed: &PendingRegistration) -> OwedRef {
    OwedRef::Registration {
        policy_id: owed.policy_id.clone(),
        resource: owed.resource_name.clone(),
        object: owed.doc_id.clone(),
    }
}

fn registration_entry(owed: PendingRegistration) -> OwedEntry {
    OwedEntry {
        what: OwedWhat::Registration,
        actor: owed.creator,
        policy_id: owed.policy_id,
        resource: owed.resource_name,
        object: owed.doc_id,
        txn: owed.txn,
        sendable: owed.state == OwedState::Staged,
        state: owed.state,
        reason: owed.last_error,
        attempts: owed.attempts,
        held_by: None,
        authority: owed.authority.as_ref().map(Into::into),
    }
}

impl<S> ExternalDocumentAcp<S>
where
    S: Store + 'static,
{
    fn not_found(entry: &OwedRef, actor: &Did) -> Error {
        Error::OwedEntryNotFound {
            entry: entry.to_string(),
            actor: actor.to_string(),
        }
    }

    fn same_actor(&self, left: &Did, right: &Did) -> bool {
        self.provider.canonical_actor(left) == self.provider.canonical_actor(right)
    }

    /// What is owed to the provider that `filter` names: registrations
    /// first, then changes in queue order.
    pub async fn entries(&self, filter: OwedFilter<'_>) -> Result<Vec<OwedEntry>> {
        let mut entries = Vec::new();
        let mut registrations = RapidHashMap::new();
        match filter {
            OwedFilter::Entry(OwedRef::Change { .. }) => {}
            OwedFilter::Entry(OwedRef::Registration {
                policy_id,
                resource,
                object,
            }) => entries.extend(
                self.owed(policy_id, resource, object)
                    .await?
                    .map(registration_entry),
            ),
            OwedFilter::Actor(_) | OwedFilter::Transaction(_) => {
                let mut start = PENDING_PREFIX.to_vec();
                loop {
                    let page = self.owed_page(&start).await?;
                    let Some(last) = page.last() else {
                        break;
                    };
                    start = last.key();
                    start.push(0);
                    entries.extend(
                        page.into_iter()
                            .filter(|owed| match filter {
                                OwedFilter::Actor(actor) => self.same_actor(&owed.creator, actor),
                                OwedFilter::Transaction(txn) => owed.txn == Some(txn),
                                OwedFilter::Entry(_) => false,
                            })
                            .map(registration_entry),
                    );
                }
            }
        }
        let wanted = |change: &OwedChange| match filter {
            OwedFilter::Actor(actor) => change.actor == self.provider.canonical_actor(actor),
            OwedFilter::Transaction(txn) => change.txn == Some(txn),
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
        Ok(entries)
    }

    /// `change` as it is reported: with what it waits behind, and whether
    /// the node can still send it. `registrations` remembers what each
    /// object owes.
    async fn change_entry(
        &self,
        change: OwedChange,
        registrations: &mut RapidHashMap<ObjectKey, Option<PendingRegistration>>,
    ) -> Result<OwedEntry> {
        let tuple = &change.tuple;
        let object = object_of(tuple);
        if !registrations.contains_key(&object) {
            let owed = self
                .owed(&tuple.policy_id, &tuple.resource, &tuple.object)
                .await?;
            registrations.insert(object.clone(), owed);
        }
        let staged = change.state == OwedState::Staged;
        let (held_by, sendable) = match registrations.get(&object).and_then(Option::as_ref) {
            Some(owed) => (
                Some(registration_ref(owed)),
                staged && owed.state == OwedState::Staged,
            ),
            None => match self.oldest_on(tuple).await? {
                // An unchecked change that is parked holds nothing: a later
                // change of its tuple passes it.
                Some(oldest)
                    if oldest.seq != change.seq
                        && (oldest.authority_checked || oldest.state == OwedState::Staged) =>
                {
                    (
                        Some(OwedRef::Change { seq: oldest.seq }),
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
        })
    }

    /// Wait until what transaction `txn` owes the provider is settled or
    /// has come to rest, and say which. The first look submits what the
    /// node can still send, once; after that the wait ends when a record
    /// changes so that nothing is left to send. It returns early, with what
    /// is still pending, when the provider could not be reached: nothing on
    /// this node sends again by itself. The caller bounds the wait.
    pub async fn await_transaction(&self, txn: u64) -> Result<OwedWait> {
        let mut submitted = false;
        loop {
            // Asked for before the read, so a record that changes after it
            // is not missed.
            let changed = self.changed.notified();
            let entries = self.entries(OwedFilter::Transaction(txn)).await?;
            if !entries.iter().any(|entry| entry.sendable) {
                return Ok(OwedWait::of(entries, false));
            }
            if submitted {
                changed.await;
                continue;
            }
            submitted = true;
            if self.submit_transaction(txn, &entries).await? {
                let entries = self.entries(OwedFilter::Transaction(txn)).await?;
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
                let authority = self.provider.command_authority(
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
                let authority = self.provider.command_authority(
                    actor,
                    ProviderCommand::Relationship {
                        tuple: &change.tuple,
                        op: change.op,
                    },
                )?;
                let _pass = self.settling.lock().await;
                if !self.write(*seq, Write::Reauthorize(authority)).await? {
                    return Err(Self::not_found(entry, actor));
                }
                object_of(&change.tuple)
            }
        };
        self.submit_object(object).await?;
        Ok(match self.entries(OwedFilter::Entry(entry)).await?.pop() {
            Some(entry) => OwedOutcome::Owed(Box::new(entry)),
            None => OwedOutcome::Settled,
        })
    }

    /// Delete the record of the refused change at `seq`, which `actor`
    /// staged, without the provider holding it. The change stops holding
    /// the later changes of its tuple, which are submitted once, and a
    /// delete stops denying its subject. A change in any other state is
    /// [`Error::OwedChangeNotRefused`]; one of another actor is not found.
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
                return Err(Self::not_found(&entry, actor));
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
        if self.oldest_on(&change.tuple).await?.is_some() {
            self.submit_object(object_of(&change.tuple)).await?;
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

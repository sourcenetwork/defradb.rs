//! Staging a relationship change a transaction owes an external provider.
//!
//! The provider decides the change when it is submitted, after the commit.
//! Staging still asks first, so the refusals a local ACP gives inside the
//! transaction arrive here too in the common case: the acting DID must own
//! the object or hold a relation that manages the changed one. The check is
//! advisory. It reads the registration this transaction or an earlier one
//! still owes (its creator is the only authority on the object until the
//! provider holds it), then the provider's state as its last round trip saw
//! it.
//!
//! When the provider cannot be reached
//! ([`Error::ProviderUnavailable`], and no other error), the check cannot
//! run. An add is then staged unchecked and says so
//! ([`Staged::authority_checked`]): it grants nothing until the provider
//! takes it, and is removed if the provider refuses it. A delete is not
//! staged ([`Error::OwedCheckUnavailable`]): it would deny its subject from
//! the commit on, on the word of an actor nobody vouched for. Any other
//! error of the provider fails the call.

use std::sync::Arc;

use datastore::NamespaceView;
use identity::Did;
use storage::corekv::Store;
use zanzibar::types::Subject;

use super::changes::{OwedChanges, StagedCount};
use super::deny::{OwedDenial, OwedDenials};
use super::record::{NewOwedChange, OwedAuthority, OwedOp, OwedState, OwedSubject, OwedTuple};
use crate::dac::DocumentACP;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::pending::PendingRegistrations;
use crate::provider_command::ProviderCommand;

mod transfer;

pub use transfer::StageTransfer;

/// A relationship change as its caller asks for it.
pub struct StageRelationship<'a> {
    /// The transaction staging it, where its caller knows one.
    pub txn: Option<u64>,
    /// The DID the change is made as.
    pub actor: &'a Did,
    pub subject: Subject,
    pub policy_id: &'a str,
    pub resource: &'a str,
    pub object: &'a str,
    pub relation: &'a str,
    /// The relations whose holders may change `relation`, from the policy.
    pub managing_relations: &'a [String],
    pub op: OwedOp,
}

/// The archive a document's delete owes, as its caller asks for it.
pub struct StageArchive<'a> {
    /// The transaction deleting the document, where its caller knows one.
    pub txn: Option<u64>,
    /// The DID the delete runs as. The archive is made as the object's
    /// owner, which need not be this DID.
    pub actor: &'a Did,
    pub policy_id: &'a str,
    pub resource: &'a str,
    pub object: &'a str,
}

/// A change recorded in its transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Staged {
    /// The change's place in the queue.
    pub seq: u64,
    /// Whether the acting DID's authority was checked. `false` for an add
    /// staged while the provider could not be reached: the provider decides
    /// at submission, and the change is removed if it refuses.
    pub authority_checked: bool,
}

/// A relationship change for an external provider, decided at the network
/// edge before the transaction that stages it
/// ([`TransactionOutbox::stage_prepared`]): the actor owns the object or holds
/// a relation that manages the changed one, as [`TransactionOutbox::stage`]
/// asks, and the authority recorded is the one the provider takes for exactly
/// this change ([`DocumentACP::command_authority`]). Nothing remote is asked
/// inside the transaction.
pub struct PreparedRelationship {
    provider: Arc<dyn DocumentACP>,
    outbox_provider: Arc<dyn DocumentACP>,
    actor: Did,
    tuple: OwedTuple,
    op: OwedOp,
    authority: super::record::OwedAuthority,
    /// The actor owns the object: a denial that spares only the owner does
    /// not refuse it.
    owner: bool,
    /// The provider answered the check; `false` for an add prepared while it
    /// could not be reached.
    checked: bool,
}

/// Decide `request` for the external provider behind `provider`, outside any
/// transaction. A refusal is the error a local ACP gives; an unreachable
/// provider lets an add go unchecked and refuses a delete, as staging does.
pub async fn prepare_relationship(
    provider: Arc<dyn DocumentACP>,
    request: &StageRelationship<'_>,
) -> Result<PreparedRelationship> {
    let outbox_provider = provider;
    let provider = outbox_provider.outbox_authority_provider().ok_or_else(|| {
        Error::ProviderUnavailable("document ACP does not provide an outbox".into())
    })?;
    if !provider.submits_subject(&request.subject) {
        return Err(Error::UnsupportedSubject(request.subject.to_string()));
    }
    let actor = provider.canonical_actor(request.actor);
    let tuple = owed_tuple(&*provider, request)?;
    let authority = provider.command_authority(
        request.actor,
        ProviderCommand::Relationship {
            tuple: &tuple,
            op: request.op,
        },
    )?;
    let (owner, checked) = prepared_authority(&*provider, &actor, request).await?;
    Ok(PreparedRelationship {
        provider,
        outbox_provider,
        actor,
        tuple,
        op: request.op,
        authority,
        owner,
        checked,
    })
}

/// Whether `actor` owns the object, and whether the provider could be asked:
/// the owner, or a holder of a relation that manages the changed one.
async fn prepared_authority(
    provider: &dyn DocumentACP,
    actor: &Did,
    request: &StageRelationship<'_>,
) -> Result<(bool, bool)> {
    let (policy_id, resource, object) = (request.policy_id, request.resource, request.object);
    let owner = match provider.get_doc_owner(policy_id, resource, object).await {
        Ok(owner) => owner.map(|owner| provider.canonical_actor(&owner)),
        Err(Error::ProviderUnavailable(reason)) => {
            return unchecked(actor, request, &reason).map(|checked| (false, checked))
        }
        Err(error) => return Err(error),
    };
    match owner {
        Some(owner) if owner == *actor => return Ok((true, true)),
        None => return Err(refusal(request)),
        Some(_) => {}
    }
    for manager in request.managing_relations {
        match provider
            .check_named_permission(actor, manager, policy_id, resource, object)
            .await
        {
            Ok(true) => return Ok((false, true)),
            Ok(false) => {}
            Err(Error::ProviderUnavailable(reason)) => {
                return unchecked(actor, request, &reason).map(|checked| (false, checked))
            }
            Err(error) => return Err(error),
        }
    }
    Err(refusal(request))
}

/// The change `request` names, its actor subject in `provider`'s spelling.
fn owed_tuple(provider: &dyn DocumentACP, request: &StageRelationship<'_>) -> Result<OwedTuple> {
    let subject = match OwedSubject::try_from(request.subject.clone())? {
        OwedSubject::Actor { did } => OwedSubject::Actor {
            did: provider.canonical_actor(&did),
        },
        other => other,
    };
    Ok(OwedTuple {
        policy_id: request.policy_id.to_owned(),
        resource: request.resource.to_owned(),
        object: request.object.to_owned(),
        relation: request.relation.to_owned(),
        subject,
    })
}

impl PreparedRelationship {
    pub fn outbox_provider(&self) -> Arc<dyn DocumentACP> {
        Arc::clone(&self.outbox_provider)
    }
}

/// The relationship changes one transaction owes an external provider, as
/// the transaction stages them. A savepoint needs the handle dropped, as it
/// needs every view of the transaction gone.
pub struct TransactionOutbox {
    provider: Arc<dyn DocumentACP>,
    changes: OwedChanges,
    pending: PendingRegistrations,
    denials: OwedDenials,
    view: datastore::WeakNamespaceView,
    /// Told once a transaction that staged an archive commits: the node's
    /// drainer, which sends what is owed by itself.
    wake: Option<Arc<regolith::sync::Notify>>,
}

impl TransactionOutbox {
    /// Record `request` with the decision `prepared` made for it before the
    /// transaction. The preparation is taken only for its own provider,
    /// actor, change and authority; what this transaction or an earlier one
    /// owes on the object is read here, so a registration another creator
    /// owes, or a revocation owed against the actor, still refuses it.
    pub async fn stage_prepared(
        &self,
        request: StageRelationship<'_>,
        prepared: &PreparedRelationship,
    ) -> Result<Staged> {
        let actor = self.provider.canonical_actor(request.actor);
        let tuple = owed_tuple(&*self.provider, &request)?;
        let authority = self.provider.command_authority(
            request.actor,
            ProviderCommand::Relationship {
                tuple: &tuple,
                op: request.op,
            },
        )?;
        if !Arc::ptr_eq(&prepared.provider, &self.provider)
            || prepared.actor != actor
            || prepared.tuple != tuple
            || prepared.op != request.op
            || authority != prepared.authority
        {
            return Err(refusal(&request));
        }
        if let Some(pending) = self
            .pending
            .get(request.policy_id, request.resource, request.object)
            .await?
        {
            if self.provider.canonical_actor(&pending.creator) != actor {
                return Err(refusal(&request));
            }
        }
        let denial = self
            .denials
            .on(
                &*self.provider,
                &Identity::Authenticated(actor.clone()),
                request.policy_id,
                request.resource,
                request.object,
            )
            .await?;
        if matches!(denial, OwedDenial::Always | OwedDenial::Archived)
            || (denial == OwedDenial::UnlessOwner
                && !prepared.owner
                && self.authority_revoked(&actor, &request).await?)
        {
            return Err(Error::PermissionDenied(format!(
                "a revocation owed to the provider denies {actor} on {} {}",
                request.resource, request.object
            )));
        }
        let change = self
            .changes
            .stage(NewOwedChange {
                txn: request.txn,
                tuple,
                op: request.op,
                actor,
                authority,
                authority_checked: prepared.checked,
            })
            .await?;
        Ok(Staged {
            seq: change.seq,
            authority_checked: prepared.checked,
        })
    }
    /// `provider` is the external provider itself, serving the request that
    /// stages; `view` the transaction's ACP namespace, `sequences` the
    /// store's and `staged` the transaction's count.
    pub(crate) fn new(
        provider: Arc<dyn DocumentACP>,
        view: &NamespaceView,
        sequences: Arc<dyn Store>,
        staged: StagedCount,
    ) -> Self {
        Self {
            provider,
            changes: OwedChanges::new(view, sequences, staged),
            pending: PendingRegistrations::new(view),
            denials: OwedDenials::new(view),
            view: view.downgrade(),
            wake: None,
        }
    }

    /// This outbox telling `wake` when a transaction that staged an archive
    /// commits.
    pub(crate) fn waking(mut self, wake: Arc<regolith::sync::Notify>) -> Self {
        self.wake = Some(wake);
        self
    }

    /// Whether a delete owed on the object may take away `actor`'s authority
    /// to make `request`: one of a relation the managers of the changed
    /// relation are computed from ([`zanzibar::types::Policy::management_sources`]),
    /// naming `actor` or a subject wider than one actor, whoever staged it
    /// (`proofs/tla/OwedStagingAuthority.tla`). Without the policy here, any
    /// such delete may.
    async fn authority_revoked(
        &self,
        actor: &Did,
        request: &StageRelationship<'_>,
    ) -> Result<bool> {
        let Some(view) = self.view.upgrade() else {
            return Ok(false);
        };
        let store = crate::zanzibar::store::TransactionZanzibarStore::new(view.clone());
        let sources = crate::ZanzibarStore::get_policy(&store, request.policy_id)
            .await?
            .map(|policy| policy.management_sources(request.resource, request.relation));
        let deletes =
            super::ops::owed_deletes_on(&view, request.policy_id, request.resource, request.object)
                .await?;
        Ok(rests_on_revoked(
            sources.as_deref(),
            &deletes,
            &self.provider.canonical_actor(actor),
        ))
    }

    /// Whether `subject`'s `relation` on the object has a change owed to the
    /// provider that this transaction sees: the provider's answer about that
    /// tuple does not hold yet.
    pub async fn relationship_unsettled(
        &self,
        policy_id: &str,
        resource: &str,
        object: &str,
        relation: &str,
        subject: &Did,
    ) -> Result<bool> {
        self.changes
            .tuple_unsettled(&OwedTuple {
                policy_id: policy_id.to_owned(),
                resource: resource.to_owned(),
                object: object.to_owned(),
                relation: relation.to_owned(),
                subject: OwedSubject::Actor {
                    did: self.provider.canonical_actor(subject),
                },
            })
            .await
    }

    /// The owed changes as this transaction sees them.
    pub fn changes(&self) -> &OwedChanges {
        &self.changes
    }

    /// How many changes this transaction has staged and kept so far.
    pub fn staged(&self) -> u32 {
        self.changes.staged()
    }

    /// Record, in this transaction, the reader a create's genesis asks for:
    /// an add by the document's creator, behind the registration this
    /// transaction owes for it. The grant is counted with that registration,
    /// one per created document, so it is held to none of the bounds on new
    /// changes and leaves the transaction's count as it was: a batch of
    /// creates that its registrations allow is never refused for its grants
    /// (`proofs/tla/RemoteGenesisReader.tla`, `Red_Counted`). Anything else
    /// is refused as [`Self::stage`] would refuse it: another op, an acting
    /// DID that is not the owed registration's creator, or an object with no
    /// owed registration.
    pub async fn stage_with_registration(&self, request: StageRelationship<'_>) -> Result<Staged> {
        if !self.provider.submits_subject(&request.subject) {
            return Err(Error::UnsupportedSubject(request.subject.to_string()));
        }
        let actor = self.provider.canonical_actor(request.actor);
        let owed = self
            .pending
            .get(request.policy_id, request.resource, request.object)
            .await?;
        if request.op != OwedOp::Add
            || owed.is_none_or(|owed| self.provider.canonical_actor(&owed.creator) != actor)
        {
            return Err(refusal(&request));
        }
        let tuple = owed_tuple(&*self.provider, &request)?;
        let authority = self.provider.command_authority(
            request.actor,
            ProviderCommand::Relationship {
                tuple: &tuple,
                op: request.op,
            },
        )?;
        let change = self
            .changes
            .stage_with_registration(NewOwedChange {
                txn: request.txn,
                tuple,
                op: request.op,
                actor,
                authority,
                authority_checked: true,
            })
            .await?;
        Ok(Staged {
            seq: change.seq,
            authority_checked: true,
        })
    }

    /// Record, in this transaction, the change `request` asks for.
    ///
    /// The acting DID and an actor subject are kept in the provider's one
    /// spelling of them. The change records what will let the node submit it
    /// as its acting DID ([`DocumentACP::command_authority`]: a delegation
    /// bound to one request counts only when it names this change); with
    /// nothing to record the call fails with
    /// [`Error::OwedAuthorityUnavailable`] and stages nothing. A refusal of
    /// the advisory check is the error a local ACP gives. A subject of a
    /// kind the provider cannot be sent is refused with
    /// [`Error::UnsupportedSubject`] before anything is recorded: such a
    /// change could never be submitted.
    pub async fn stage(&self, request: StageRelationship<'_>) -> Result<Staged> {
        if !self.provider.submits_subject(&request.subject) {
            return Err(Error::UnsupportedSubject(request.subject.to_string()));
        }
        let actor = self.provider.canonical_actor(request.actor);
        let subject = match OwedSubject::try_from(request.subject.clone())? {
            OwedSubject::Actor { did } => OwedSubject::Actor {
                did: self.provider.canonical_actor(&did),
            },
            other => other,
        };
        let tuple = OwedTuple {
            policy_id: request.policy_id.to_owned(),
            resource: request.resource.to_owned(),
            object: request.object.to_owned(),
            relation: request.relation.to_owned(),
            subject,
        };
        let authority = self.provider.command_authority(
            request.actor,
            ProviderCommand::Relationship {
                tuple: &tuple,
                op: request.op,
            },
        )?;
        let authority_checked = self.advise(&actor, &request).await?;
        let change = self
            .changes
            .stage(NewOwedChange {
                txn: request.txn,
                tuple,
                op: request.op,
                actor,
                authority,
                authority_checked,
            })
            .await?;
        Ok(Staged {
            seq: change.seq,
            authority_checked,
        })
    }

    /// Record, in the delete's transaction, the archive of its document's
    /// object (`proofs/tla/ArchiveOnDelete.tla`).
    ///
    /// The archive is made as the object's owner, whoever deletes: the
    /// creator of a registration still owed, or the owner the provider
    /// names. Whether `request.actor` may delete at all is decided before
    /// this is asked; an owner is not required of it. The node submits the
    /// archive only under the owner's authority
    /// ([`DocumentACP::command_authority`] for the owner: a key the node
    /// holds for it, or a delegation the request carries for it); when it
    /// has none the archive is parked as awaiting the owner's authority. It
    /// denies all the same, and is sent once the owner hands in a credential
    /// (`reauthorize_owed`). An object nobody owns is refused
    /// ([`Error::NotOwner`]): there is no one to archive it as. While the
    /// provider cannot say who the owner is the delete is refused too
    /// ([`Error::OwedCheckUnavailable`]): the archive denies everyone on the
    /// object from the commit on. Earlier changes on the object not yet
    /// handed to the provider's worker are dropped with the delete, each
    /// with a note for its actor: the archive takes away whatever they
    /// would have done. A change already in flight is sent before the
    /// archive, and the provider refuses any change to an archived object.
    pub async fn stage_archive(&self, request: StageArchive<'_>) -> Result<Staged> {
        let deleter = self.provider.canonical_actor(request.actor);
        let (policy_id, resource, object) = (request.policy_id, request.resource, request.object);
        let owner = match self.pending.get(policy_id, resource, object).await? {
            Some(owed) => self.provider.canonical_actor(&owed.creator),
            None => match self
                .provider
                .get_doc_owner(policy_id, resource, object)
                .await
            {
                Ok(Some(owner)) => self.provider.canonical_actor(&owner),
                Ok(None) => {
                    return Err(Error::NotOwner {
                        operation: "archive the deleted document's object, which has no owner"
                            .into(),
                    })
                }
                Err(Error::ProviderUnavailable(_)) => {
                    return Err(Error::OwedCheckUnavailable {
                        actor: deleter.to_string(),
                        object: object.to_owned(),
                    })
                }
                Err(error) => return Err(error),
            },
        };
        let deleted_by_owner = owner == deleter;
        let tuple = OwedTuple::archive(policy_id, resource, object);
        // The owner's own request is asked in the spelling it was made in;
        // anyone else's request carries nothing for the owner, so only a
        // key the node holds for it counts.
        let acting = if deleted_by_owner {
            request.actor
        } else {
            &owner
        };
        // When nothing lets the node sign for the owner now (no key, no
        // delegation in the owner's request), the archive waits for its
        // authority, denying everyone on the object until the owner hands
        // one in (`reauthorize_owed`).
        let (authority, awaiting) = match self.provider.command_authority(
            acting,
            ProviderCommand::Relationship {
                tuple: &tuple,
                op: OwedOp::Archive,
            },
        ) {
            Ok(authority) => (authority, None),
            Err(error @ Error::OwedAuthorityUnavailable { .. }) => {
                (OwedAuthority::NodeKey, Some(error.to_string()))
            }
            Err(error) => return Err(error),
        };
        self.changes
            .drop_unsent_on(policy_id, resource, object)
            .await?;
        // One archive per deleted document: the delete bounds them, and a
        // delete is not refused for the outbox's bounds on new changes.
        let change = self
            .changes
            .stage_archive(
                NewOwedChange {
                    txn: request.txn,
                    tuple,
                    op: OwedOp::Archive,
                    actor: owner,
                    authority,
                    authority_checked: true,
                },
                deleted_by_owner,
            )
            .await?;
        if let Some(reason) = awaiting {
            self.changes
                .mark(change.seq, OwedState::AwaitingAuthority, 0, Some(reason))
                .await?;
        }
        // A delete has no hook of its own that settles what it staged: the
        // drainer is told when it commits.
        if let (Some(wake), Some(view)) = (self.wake.clone(), self.view.upgrade()) {
            view.on_commit(storage::corekv::OnCommit::Sync(Box::new(move || {
                wake.notify_one()
            })))
            .await
            .map_err(|error| Error::Storage(error.to_string()))?;
        }
        Ok(Staged {
            seq: change.seq,
            authority_checked: true,
        })
    }

    /// Whether `actor` may make the change, as far as can be known now.
    /// `Ok(false)` for an add the provider could not be asked about.
    async fn advise(&self, actor: &Did, request: &StageRelationship<'_>) -> Result<bool> {
        let (policy_id, resource, object) = (request.policy_id, request.resource, request.object);
        let denial = self
            .denials
            .on(
                &*self.provider,
                &Identity::Authenticated(actor.clone()),
                policy_id,
                resource,
                object,
            )
            .await?;
        let revoked = || {
            Error::PermissionDenied(format!(
                "a revocation owed to the provider denies {actor} on {resource} {object}"
            ))
        };
        // An owed transfer away from the actor, or an owed archive, denies
        // it before a registration still owed to it could let it in: a
        // creator that gave its document away stages nothing more on it.
        if matches!(denial, OwedDenial::Always | OwedDenial::Archived) {
            return Err(revoked());
        }
        if let Some(owed) = self.pending.get(policy_id, resource, object).await? {
            return if self.provider.canonical_actor(&owed.creator) == *actor {
                Ok(true)
            } else {
                Err(refusal(request))
            };
        }
        let owner = match self
            .provider
            .get_doc_owner(policy_id, resource, object)
            .await
        {
            Ok(owner) => owner.map(|owner| self.provider.canonical_actor(&owner)),
            // A denial that spares only the owner stands while nobody can
            // say who the owner is.
            Err(Error::ProviderUnavailable(_))
                if denial == OwedDenial::UnlessOwner
                    && self.authority_revoked(actor, request).await? =>
            {
                return Err(revoked())
            }
            Err(Error::ProviderUnavailable(reason)) => return unchecked(actor, request, &reason),
            Err(error) => return Err(error),
        };
        match owner {
            Some(owner) if owner == *actor => return Ok(true),
            _ if denial == OwedDenial::UnlessOwner
                && self.authority_revoked(actor, request).await? =>
            {
                return Err(revoked())
            }
            None => return Err(refusal(request)),
            Some(_) => {}
        }
        for manager in request.managing_relations {
            match self
                .provider
                .check_named_permission(actor, manager, policy_id, resource, object)
                .await
            {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(Error::ProviderUnavailable(reason)) => {
                    return unchecked(actor, request, &reason)
                }
                Err(error) => return Err(error),
            }
        }
        Err(refusal(request))
    }
}

/// Whether one of `deletes` takes away what an actor's authority rests on: a
/// delete of one of `sources` (any relation when they are unknown) naming
/// `actor` or a subject wider than one actor.
fn rests_on_revoked(sources: Option<&[String]>, deletes: &[OwedTuple], actor: &Did) -> bool {
    deletes.iter().any(|delete| {
        sources.is_none_or(|sources| sources.contains(&delete.relation))
            && match &delete.subject {
                OwedSubject::Actor { did } => did == actor,
                _ => true,
            }
    })
}

/// The refusal a local ACP gives an actor that neither owns the object nor
/// manages the relation.
fn refusal(request: &StageRelationship<'_>) -> Error {
    let verb = match request.op {
        OwedOp::Add => "create",
        OwedOp::Delete | OwedOp::Archive | OwedOp::Transfer => "delete",
    };
    if request.managing_relations.is_empty() {
        Error::NotOwner {
            operation: format!("{verb} actor relationship"),
        }
    } else {
        Error::NotManager {
            operation: format!("{verb} relationship"),
        }
    }
}

/// What staging does when the provider cannot be asked: an add goes on
/// unchecked, a delete does not go on.
fn unchecked(actor: &Did, request: &StageRelationship<'_>, reason: &str) -> Result<bool> {
    if request.op != OwedOp::Add {
        return Err(Error::OwedCheckUnavailable {
            actor: actor.to_string(),
            object: request.object.to_owned(),
        });
    }
    tracing::warn!(
        target: "acp::audit",
        %actor,
        object = request.object,
        reason,
        "ACP provider unavailable; relationship add staged without the authority check"
    );
    Ok(false)
}

#[cfg(test)]
mod authority_tests {
    use super::*;

    fn delete(relation: &str, subject: OwedSubject) -> OwedTuple {
        OwedTuple {
            policy_id: "p".into(),
            resource: "users".into(),
            object: "doc".into(),
            relation: relation.into(),
            subject,
        }
    }

    fn actor(name: &str) -> Did {
        Did::new(format!("did:key:{name}")).unwrap()
    }

    #[test]
    fn only_a_revoked_source_naming_the_actor_or_everyone_takes_its_authority() {
        let sources = ["manager".to_string()];
        let mallory = actor("mallory");
        let named = |relation, who: &str| delete(relation, OwedSubject::Actor { did: actor(who) });
        assert!(rests_on_revoked(
            Some(&sources),
            &[named("manager", "mallory")],
            &mallory
        ));
        assert!(rests_on_revoked(
            Some(&sources),
            &[delete("manager", OwedSubject::Wildcard)],
            &mallory
        ));
        assert!(!rests_on_revoked(
            Some(&sources),
            &[named("manager", "bob")],
            &mallory
        ));
        assert!(!rests_on_revoked(
            Some(&sources),
            &[delete("reader", OwedSubject::Wildcard)],
            &mallory
        ));
        assert!(!rests_on_revoked(
            Some(&sources),
            &[named("reader", "mallory")],
            &mallory
        ));
        assert!(
            rests_on_revoked(None, &[delete("reader", OwedSubject::Wildcard)], &mallory),
            "without the policy any wide delete may"
        );
    }
}

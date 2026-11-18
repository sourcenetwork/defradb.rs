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
use super::record::{NewOwedChange, OwedOp, OwedSubject, OwedTuple};
use crate::dac::DocumentACP;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::pending::PendingRegistrations;

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

pub struct PreparedOwnerRelationship {
    provider: Arc<dyn DocumentACP>,
    outbox_provider: Arc<dyn DocumentACP>,
    actor: Did,
    tuple: OwedTuple,
    op: OwedOp,
    authority: super::record::OwedAuthority,
}

pub async fn prepare_owner_relationship(
    provider: Arc<dyn DocumentACP>,
    request: &StageRelationship<'_>,
) -> Result<PreparedOwnerRelationship> {
    let outbox_provider = provider;
    let provider = outbox_provider.outbox_authority_provider().ok_or_else(|| {
        Error::ProviderUnavailable("document ACP does not provide an outbox".into())
    })?;
    if !provider.submits_subject(&request.subject) {
        return Err(Error::UnsupportedSubject(request.subject.to_string()));
    }
    let actor = provider.canonical_actor(request.actor);
    let owner = provider
        .get_doc_owner(request.policy_id, request.resource, request.object)
        .await?
        .map(|owner| provider.canonical_actor(&owner));
    if owner.as_ref() != Some(&actor) {
        return Err(refusal(request));
    }
    let subject = match OwedSubject::try_from(request.subject.clone())? {
        OwedSubject::Actor { did } => OwedSubject::Actor {
            did: provider.canonical_actor(&did),
        },
        other => other,
    };
    let authority = provider.owed_authority(request.actor)?;
    Ok(PreparedOwnerRelationship {
        provider,
        outbox_provider,
        actor,
        tuple: OwedTuple {
            policy_id: request.policy_id.to_owned(),
            resource: request.resource.to_owned(),
            object: request.object.to_owned(),
            relation: request.relation.to_owned(),
            subject,
        },
        op: request.op,
        authority,
    })
}

impl PreparedOwnerRelationship {
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
}

impl TransactionOutbox {
    pub async fn stage_prepared_owner(
        &self,
        request: StageRelationship<'_>,
        prepared: &PreparedOwnerRelationship,
    ) -> Result<Staged> {
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
        if !Arc::ptr_eq(&prepared.provider, &self.provider)
            || prepared.actor != actor
            || prepared.tuple != tuple
            || prepared.op != request.op
            || self.provider.owed_authority(request.actor)? != prepared.authority
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
        if denial == OwedDenial::Always {
            return Err(refusal(&request));
        }
        let change = self
            .changes
            .stage(NewOwedChange {
                txn: request.txn,
                tuple,
                op: request.op,
                actor,
                authority: prepared.authority.clone(),
                authority_checked: true,
            })
            .await?;
        Ok(Staged {
            seq: change.seq,
            authority_checked: true,
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
        }
    }

    /// The owed changes as this transaction sees them.
    pub fn changes(&self) -> &OwedChanges {
        &self.changes
    }

    /// How many changes this transaction has staged and kept so far.
    pub fn staged(&self) -> u32 {
        self.changes.staged()
    }

    /// Record, in this transaction, the change `request` asks for.
    ///
    /// The acting DID and an actor subject are kept in the provider's one
    /// spelling of them. The change records what will let the node submit it
    /// as its acting DID; with nothing to record the call fails with
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
        let authority = self.provider.owed_authority(request.actor)?;
        let authority_checked = self.advise(&actor, &request).await?;
        let change = self
            .changes
            .stage(NewOwedChange {
                txn: request.txn,
                tuple: OwedTuple {
                    policy_id: request.policy_id.to_owned(),
                    resource: request.resource.to_owned(),
                    object: request.object.to_owned(),
                    relation: request.relation.to_owned(),
                    subject,
                },
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

    /// Whether `actor` may make the change, as far as can be known now.
    /// `Ok(false)` for an add the provider could not be asked about.
    async fn advise(&self, actor: &Did, request: &StageRelationship<'_>) -> Result<bool> {
        let (policy_id, resource, object) = (request.policy_id, request.resource, request.object);
        if let Some(owed) = self.pending.get(policy_id, resource, object).await? {
            return if self.provider.canonical_actor(&owed.creator) == *actor {
                Ok(true)
            } else {
                Err(refusal(request))
            };
        }
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
        if denial == OwedDenial::Always {
            return Err(revoked());
        }
        let owner = match self
            .provider
            .get_doc_owner(policy_id, resource, object)
            .await
        {
            Ok(owner) => owner.map(|owner| self.provider.canonical_actor(&owner)),
            // A denial that spares only the owner stands while nobody can
            // say who the owner is.
            Err(Error::ProviderUnavailable(_)) if denial == OwedDenial::UnlessOwner => {
                return Err(revoked())
            }
            Err(Error::ProviderUnavailable(reason)) => return unchecked(actor, request, &reason),
            Err(error) => return Err(error),
        };
        match owner {
            Some(owner) if owner == *actor => return Ok(true),
            _ if denial == OwedDenial::UnlessOwner => return Err(revoked()),
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

/// The refusal a local ACP gives an actor that neither owns the object nor
/// manages the relation.
fn refusal(request: &StageRelationship<'_>) -> Error {
    let verb = match request.op {
        OwedOp::Add => "create",
        OwedOp::Delete => "delete",
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
    if request.op == OwedOp::Delete {
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

//! Staging a relationship change a transaction owes an external provider.
//!
//! The provider decides the change when it is submitted, after the commit.
//! Staging still asks first, so the refusals a local ACP gives inside the
//! transaction arrive here too in the common case: the acting DID must own
//! the object or hold a relation that manages the changed one. The check is
//! advisory. It reads the registration this transaction or an earlier one
//! still owes (its creator is the only authority on the object until the
//! provider holds it), then the provider's state as its last round trip saw
//! it. When the provider cannot answer, the change is staged unchecked and
//! says so ([`Staged::authority_checked`]): commits keep succeeding while the
//! provider is down, and the provider refuses at submission what it would
//! have refused here.

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
    /// Whether the acting DID's authority was checked against the provider.
    /// `false` when the provider could not answer: the change is staged all
    /// the same and the provider decides at submission.
    pub authority_checked: bool,
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
    /// the advisory check is the error a local ACP gives.
    pub async fn stage(&self, request: StageRelationship<'_>) -> Result<Staged> {
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
            })
            .await?;
        Ok(Staged {
            seq: change.seq,
            authority_checked,
        })
    }

    /// Whether `actor` may make the change, as far as can be known now.
    /// `Ok(false)` when the provider could not say.
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
            Err(_) if denial == OwedDenial::UnlessOwner => return Err(revoked()),
            Err(error) => return Ok(unchecked(actor, object, &error)),
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
                Err(error) => return Ok(unchecked(actor, object, &error)),
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

fn unchecked(actor: &Did, object: &str, error: &Error) -> bool {
    tracing::warn!(
        target: "acp::audit",
        %actor,
        object,
        %error,
        "ACP provider did not answer; relationship change staged without the advisory authority check"
    );
    false
}

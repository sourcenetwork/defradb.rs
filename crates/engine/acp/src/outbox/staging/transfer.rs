//! Staging the transfer of an object's ownership to another actor
//! (`proofs/tla/OwnerTransfer.tla`).
//!
//! Only the object's owner may give it away: the creator of a registration
//! still owed (the provider names nobody else before it holds the
//! registration), or the owner the provider names. The check is the
//! provider's own rule ahead of time, so the common refusal arrives before
//! the commit. A transfer denies its actor on the object from its commit
//! until the provider holds it, so it is never staged unchecked: while the
//! provider cannot say who the owner is, it is refused
//! ([`Error::OwedCheckUnavailable`]).

use identity::Did;

use super::{Staged, TransactionOutbox};
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::outbox::deny::OwedDenial;
use crate::outbox::record::{NewOwedChange, OwedOp, OwedTuple};
use crate::provider_command::ProviderCommand;

/// The transfer of one object to another owner, as its caller asks for it.
pub struct StageTransfer<'a> {
    /// The transaction staging it, where its caller knows one.
    pub txn: Option<u64>,
    /// The DID the transfer is made as: the object's owner.
    pub actor: &'a Did,
    /// The DID that owns the object once the provider holds the transfer.
    pub new_owner: &'a Did,
    pub policy_id: &'a str,
    pub resource: &'a str,
    pub object: &'a str,
}

/// The refusal a transfer by an actor that does not own the object gets.
fn not_owner() -> Error {
    Error::NotOwner {
        operation: "transfer the document's ownership".into(),
    }
}

impl TransactionOutbox {
    /// Record, in this transaction, the transfer `request` asks for.
    /// `None` when the actor gives the object to itself: it owns it already,
    /// and nothing is owed.
    ///
    /// The acting DID and the new owner are kept in the provider's one
    /// spelling of them. An acting DID that does not own the object is
    /// refused with [`Error::NotOwner`], one an owed transfer or archive
    /// already denies on the object with [`Error::PermissionDenied`], and a
    /// new owner that is not one actor with [`Error::UnsupportedSubject`].
    /// The change records what will let the node submit it as the acting
    /// DID ([`crate::DocumentACP::command_authority`]); with nothing to
    /// record the call fails with [`Error::OwedAuthorityUnavailable`] and
    /// stages nothing.
    pub async fn stage_transfer(&self, request: StageTransfer<'_>) -> Result<Option<Staged>> {
        let (policy_id, resource, object) = (request.policy_id, request.resource, request.object);
        if request.new_owner.is_wildcard() {
            return Err(Error::UnsupportedSubject(request.new_owner.to_string()));
        }
        let actor = self.provider.canonical_actor(request.actor);
        let new_owner = self.provider.canonical_actor(request.new_owner);
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
        if matches!(denial, OwedDenial::Always | OwedDenial::Archived) {
            return Err(Error::PermissionDenied(format!(
                "a change owed to the provider denies {actor} on {resource} {object}"
            )));
        }
        // The provider's owner is asked here, inside the staging statement,
        // as `advise` asks it for a relationship change: a known divergence
        // from the house rule that nothing remote is fetched inside a
        // transaction's closure. The answer is advisory and decides no
        // transactional read: the transaction cannot read the provider, a
        // retried attempt asks again, and the provider checks the owner
        // itself when the transfer lands, refusing it (kept and reported)
        // if the owner changed meanwhile. Routing it through the prepared
        // input memo is left to the same change for every staged
        // relationship change.
        let owner = match self.pending.get(policy_id, resource, object).await? {
            Some(owed) => self.provider.canonical_actor(&owed.creator),
            None => match self
                .provider
                .get_doc_owner(policy_id, resource, object)
                .await
            {
                Ok(Some(owner)) => self.provider.canonical_actor(&owner),
                Ok(None) => return Err(not_owner()),
                Err(Error::ProviderUnavailable(_)) => {
                    return Err(Error::OwedCheckUnavailable {
                        actor: actor.to_string(),
                        object: object.to_owned(),
                    })
                }
                Err(error) => return Err(error),
            },
        };
        if owner != actor {
            return Err(not_owner());
        }
        if new_owner == actor {
            return Ok(None);
        }
        let tuple = OwedTuple::transfer(policy_id, resource, object, new_owner);
        let authority = self.provider.command_authority(
            request.actor,
            ProviderCommand::Relationship {
                tuple: &tuple,
                op: OwedOp::Transfer,
            },
        )?;
        let change = self
            .changes
            .stage(NewOwedChange {
                txn: request.txn,
                tuple,
                op: OwedOp::Transfer,
                actor,
                authority,
                authority_checked: true,
            })
            .await?;
        Ok(Some(Staged {
            seq: change.seq,
            authority_checked: true,
        }))
    }
}

//! Whom an owed delete denies.
//!
//! An owed archive (its document was deleted) denies everyone on its
//! object, its owner and anonymous callers too, from the delete's commit
//! until the provider holds it: a deleted document grants nothing
//! (`proofs/tla/ArchiveOnDelete.tla`). One read is left: the actor that
//! staged the archive still reads the document's commit history.
//!
//! While any delete of a tuple is unsettled, its subject is denied on its
//! object before the provider is asked, whatever the provider still holds:
//! the revocation takes effect on this node when it commits. An add staged
//! after the delete does not lift the denial: the provider may refuse that add
//! and still apply the delete, and until it has seen the delete it grants what
//! the delete takes away. A delete the provider refused keeps denying while
//! its record is kept. The denial covers every permission on the object, so a
//! subject that also holds access by another path waits for the settlement
//! too. It only ever denies: an owed add grants nothing.

//!
//! A decision is two point reads, whatever the policy: the object's count of
//! deletes wider than one actor (a wildcard, a set), and the count of deletes
//! naming the identity on the object (any relation, and the owner relation).
//! Both are read through their staged part only: a delete committed after the
//! read refuses the decision at commit, a settle never does, and a settle the
//! read did not see only leaves it denying more (`proofs/tla/OwedParts.tla`).

use datastore::{NamespaceView, WeakNamespaceView};
use storage::corekv::Reader;

use super::keys;
use super::ops::{self, Owed, FIRST, SECOND};
use crate::dac::DocumentACP;
use crate::error::Result;
use crate::identity::Identity;
use crate::outbox::record::{OwedChange, OwedOp};
use identity::Did;

/// What the changes owed on an object say about one identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwedDenial {
    /// Nothing owed denies it.
    None,
    /// Denied unless it owns the object: a delete names it under a relation
    /// other than the owner's, or names a subject wider than one actor.
    UnlessOwner,
    /// Denied: the owner relation itself is being taken from it.
    Always,
    /// Denied: the object's archive is owed. Only the actor that staged the
    /// archive ([`OwedDenials::archive_actor`]) is spared, and only for the
    /// document's commit history.
    Archived,
}

/// The denial owed deletes put on an identity: `wide` counts the deletes on
/// the object whose subject is wider than one actor (who it stood for is the
/// provider's to say, so it denies everyone it could have stood for), and
/// `named` the deletes naming the identity, all of them and those of the
/// owner relation.
fn denial(wide: u64, named: Owed) -> OwedDenial {
    if named.second > 0 {
        OwedDenial::Always
    } else if wide > 0 || named.first > 0 {
        OwedDenial::UnlessOwner
    } else {
        OwedDenial::None
    }
}

/// The denial owed deletes put on `identity` on one object, compared in
/// `provider`'s spelling of it. Two point reads, validated on what was
/// staged only.
pub(crate) async fn owed_denial<R>(
    reader: &R,
    provider: &dyn DocumentACP,
    identity: &Identity,
    policy_id: &str,
    resource: &str,
    object: &str,
) -> Result<OwedDenial>
where
    R: Reader + ?Sized,
{
    let object_key = keys::object_key(policy_id, resource, object);
    let on_object = ops::owed_at(reader, &object_key, &ops::staged_parts(&[FIRST, SECOND])).await?;
    if on_object.second > 0 {
        return Ok(OwedDenial::Archived);
    }
    let wide = on_object.first;
    let named = match identity {
        Identity::Authenticated(actor) => {
            let actor = provider.canonical_actor(actor);
            let key = keys::subject_key(policy_id, resource, object, &actor);
            ops::owed_at(reader, &key, &ops::staged_parts(&[FIRST, SECOND])).await?
        }
        Identity::Anonymous => Owed::default(),
    };
    Ok(denial(wide, named))
}

/// The actor that staged the archive owed on one object, while it is
/// unsettled: the deleting owner may inspect its own commit history before
/// the archive settles. A change staged on that tuple commits first aborts
/// the decision.
pub(super) async fn archive_actor<R>(
    reader: &R,
    policy_id: &str,
    resource: &str,
    object: &str,
) -> Result<Option<Did>>
where
    R: Reader + ?Sized,
{
    let tuple = crate::outbox::record::OwedTuple::archive(policy_id, resource, object);
    let Some(state) = ops::tuple_state(reader, &tuple).await? else {
        return Ok(None);
    };
    let change: OwedChange = ops::change(reader, state.oldest).await?.ok_or_else(|| {
        crate::Error::Serialization(format!("owed ACP change {} has no record", state.oldest))
    })?;
    Ok(matches!(change.op, OwedOp::Archive).then_some(change.actor))
}

/// Owed denials as one transaction sees them: the deletes it has staged
/// itself over those committed and unsettled. The handle does not keep the
/// transaction in use; once that is committed, rolled back or dropped,
/// nothing is denied through it.
#[derive(Clone)]
pub struct OwedDenials {
    view: WeakNamespaceView,
}

/// The actor that staged an archive owed on one object, as this transaction
/// sees it.
impl OwedDenials {
    pub async fn archive_actor(
        &self,
        policy_id: &str,
        resource: &str,
        object: &str,
    ) -> Result<Option<Did>> {
        let Some(view) = self.view.upgrade() else {
            return Ok(None);
        };
        archive_actor(&view, policy_id, resource, object).await
    }
}

impl OwedDenials {
    /// `view` is the transaction's ACP namespace.
    pub fn new(view: &NamespaceView) -> Self {
        Self {
            view: view.downgrade(),
        }
    }

    /// The denial owed deletes put on `identity` on one object, compared in
    /// `provider`'s spelling of it ([`DocumentACP::canonical_actor`]).
    pub async fn on(
        &self,
        provider: &dyn DocumentACP,
        identity: &Identity,
        policy_id: &str,
        resource: &str,
        object: &str,
    ) -> Result<OwedDenial> {
        let Some(view) = self.view.upgrade() else {
            return Ok(OwedDenial::None);
        };
        owed_denial(&view, provider, identity, policy_id, resource, object).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(deletes: u64, owner: u64) -> Owed {
        Owed {
            first: deletes,
            second: owner,
        }
    }

    #[test]
    fn a_delete_naming_the_identity_denies_it_unless_it_owns() {
        assert_eq!(denial(0, named(1, 0)), OwedDenial::UnlessOwner);
        assert_eq!(denial(0, named(0, 0)), OwedDenial::None);
    }

    #[test]
    fn a_delete_of_the_owner_relation_spares_no_one_it_names() {
        assert_eq!(denial(0, named(1, 1)), OwedDenial::Always);
        assert_eq!(denial(3, named(2, 1)), OwedDenial::Always);
    }

    #[test]
    fn a_delete_wider_than_one_actor_denies_everyone_but_the_owner() {
        assert_eq!(denial(1, named(0, 0)), OwedDenial::UnlessOwner);
        assert_eq!(denial(1, Owed::default()), OwedDenial::UnlessOwner);
    }
}

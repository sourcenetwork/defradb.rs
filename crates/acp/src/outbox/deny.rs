//! Whom an owed delete denies.
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

use datastore::{NamespaceView, WeakNamespaceView};
use storage::corekv::Reader;

use super::ops;
use super::record::OwedSubject;
use crate::dac::DocumentACP;
use crate::error::Result;
use crate::identity::Identity;
use crate::relation::OWNER_RELATION;

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
}

/// The denial `relation` and `subject` of one owed delete put on `identity`.
fn denial_of(identity: &Identity, relation: &str, subject: &OwedSubject) -> OwedDenial {
    match (subject, identity) {
        (OwedSubject::Actor { did }, Identity::Authenticated(actor)) if did == actor => {
            if relation == OWNER_RELATION {
                OwedDenial::Always
            } else {
                OwedDenial::UnlessOwner
            }
        }
        (OwedSubject::Actor { .. }, _) => OwedDenial::None,
        // Who a wildcard or a set stood for is the provider's to say, so the
        // delete denies everyone it could have stood for.
        (
            OwedSubject::Wildcard
            | OwedSubject::EntitySet { .. }
            | OwedSubject::TypedWildcard { .. },
            _,
        ) => OwedDenial::UnlessOwner,
    }
}

/// The denial owed deletes put on `identity` on one object, compared in
/// `provider`'s spelling of it. One point read when nothing is owed on the
/// object.
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
    if ops::unsettled_on(reader, policy_id, resource, object).await? == 0 {
        return Ok(OwedDenial::None);
    }
    let identity = &match identity {
        Identity::Authenticated(actor) => Identity::Authenticated(provider.canonical_actor(actor)),
        Identity::Anonymous => Identity::Anonymous,
    };
    let mut denial = OwedDenial::None;
    for (relation, subject, state) in ops::tuples_on(reader, policy_id, resource, object).await? {
        if state.deletes == 0 {
            continue;
        }
        match denial_of(identity, &relation, &subject) {
            OwedDenial::Always => return Ok(OwedDenial::Always),
            OwedDenial::UnlessOwner => denial = OwedDenial::UnlessOwner,
            OwedDenial::None => {}
        }
    }
    Ok(denial)
}

/// Owed denials as one transaction sees them: the deletes it has staged
/// itself over those committed and unsettled. The handle does not keep the
/// transaction in use; once that is committed, rolled back or dropped,
/// nothing is denied through it.
#[derive(Clone)]
pub struct OwedDenials {
    view: WeakNamespaceView,
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
    use identity::Did;

    use super::*;

    fn did(name: &str) -> Did {
        Did::new(format!("did:key:{name}")).unwrap()
    }

    fn actor(name: &str) -> OwedSubject {
        OwedSubject::Actor { did: did(name) }
    }

    #[test]
    fn a_delete_denies_its_own_subject_only() {
        let bob = Identity::Authenticated(did("bob"));
        assert_eq!(
            denial_of(&bob, "reader", &actor("bob")),
            OwedDenial::UnlessOwner
        );
        assert_eq!(denial_of(&bob, "reader", &actor("carol")), OwedDenial::None);
        assert_eq!(
            denial_of(&Identity::Anonymous, "reader", &actor("bob")),
            OwedDenial::None
        );
    }

    #[test]
    fn a_delete_of_the_owner_relation_spares_no_one_it_names() {
        let bob = Identity::Authenticated(did("bob"));
        assert_eq!(
            denial_of(&bob, OWNER_RELATION, &actor("bob")),
            OwedDenial::Always
        );
    }

    #[test]
    fn a_delete_wider_than_one_actor_denies_everyone_but_the_owner() {
        let wide = [
            OwedSubject::Wildcard,
            OwedSubject::EntitySet {
                resource: "groups".into(),
                object: "team".into(),
                relation: "member".into(),
            },
            OwedSubject::TypedWildcard {
                resource: "groups".into(),
            },
        ];
        for subject in &wide {
            for identity in [Identity::Anonymous, Identity::Authenticated(did("bob"))] {
                assert_eq!(
                    denial_of(&identity, "reader", subject),
                    OwedDenial::UnlessOwner
                );
            }
        }
    }
}

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

use storage::corekv::Reader;

use super::ops;
use super::record::OwedSubject;
use crate::error::Result;
use crate::identity::Identity;
use crate::relation::OWNER_RELATION;

/// What the changes owed on an object say about one identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Denial {
    /// Nothing owed denies it.
    None,
    /// Denied unless it owns the object: a delete names it under a relation
    /// other than the owner's, or names a subject wider than one actor.
    UnlessOwner,
    /// Denied: the owner relation itself is being taken from it.
    Always,
}

/// The denial `relation` and `subject` of one owed delete put on `identity`.
fn denial_of(identity: &Identity, relation: &str, subject: &OwedSubject) -> Denial {
    match (subject, identity) {
        (OwedSubject::Actor { did }, Identity::Authenticated(actor)) if did == actor => {
            if relation == OWNER_RELATION {
                Denial::Always
            } else {
                Denial::UnlessOwner
            }
        }
        (OwedSubject::Actor { .. }, _) => Denial::None,
        // Who a wildcard or a set stood for is the provider's to say, so the
        // delete denies everyone it could have stood for.
        (
            OwedSubject::Wildcard
            | OwedSubject::EntitySet { .. }
            | OwedSubject::TypedWildcard { .. },
            _,
        ) => Denial::UnlessOwner,
    }
}

/// The denial owed deletes put on `identity` on one object. One point read
/// when nothing is owed on the object.
pub(crate) async fn owed_denial<R>(
    reader: &R,
    identity: &Identity,
    policy_id: &str,
    resource: &str,
    object: &str,
) -> Result<Denial>
where
    R: Reader + ?Sized,
{
    if ops::unsettled_on(reader, policy_id, resource, object).await? == 0 {
        return Ok(Denial::None);
    }
    let mut denial = Denial::None;
    for (relation, subject, state) in ops::tuples_on(reader, policy_id, resource, object).await? {
        if state.deletes == 0 {
            continue;
        }
        match denial_of(identity, &relation, &subject) {
            Denial::Always => return Ok(Denial::Always),
            Denial::UnlessOwner => denial = Denial::UnlessOwner,
            Denial::None => {}
        }
    }
    Ok(denial)
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
            Denial::UnlessOwner
        );
        assert_eq!(denial_of(&bob, "reader", &actor("carol")), Denial::None);
        assert_eq!(
            denial_of(&Identity::Anonymous, "reader", &actor("bob")),
            Denial::None
        );
    }

    #[test]
    fn a_delete_of_the_owner_relation_spares_no_one_it_names() {
        let bob = Identity::Authenticated(did("bob"));
        assert_eq!(
            denial_of(&bob, OWNER_RELATION, &actor("bob")),
            Denial::Always
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
                assert_eq!(denial_of(&identity, "reader", subject), Denial::UnlessOwner);
            }
        }
    }
}

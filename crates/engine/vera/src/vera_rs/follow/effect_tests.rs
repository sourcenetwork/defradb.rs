//! Reading back what a signed request did.

use super::super::fixtures::*;
use super::*;
use crate::SubjectRef;

#[test]
fn registrations_and_relationship_changes_are_read_back_with_their_delegation() {
    let reader = SubjectRef::Actor(READER.into());
    assert_eq!(
        effect_of(&wire(0, "bae-1", register())),
        Effect::Owed(Lost::Registration {
            policy_id: POLICY.into(),
            resource: "users".into(),
            object: "bae-1".into(),
            token: token(OWNER),
        })
    );
    for (action, add) in [(grant(&reader), true), (revoke(&reader), false)] {
        assert_eq!(
            effect_of(&wire(1, "bae-1", action)),
            Effect::Owed(Lost::Relationship {
                add,
                policy_id: POLICY.into(),
                resource: "users".into(),
                object: "bae-1".into(),
                relation: "reader".into(),
                subject: LostSubject::Actor { did: READER.into() },
                token: token(OWNER),
            })
        );
    }
    let everyone = SubjectRef::AllActors;
    assert!(matches!(
        effect_of(&wire(2, "bae-1", grant(&everyone))),
        Effect::Owed(Lost::Relationship {
            subject: LostSubject::Wildcard,
            ..
        })
    ));
}

#[test]
fn an_archive_and_other_requests_are_told_apart() {
    assert_eq!(
        effect_of(&archive(0, "bae-1")),
        Effect::Archive {
            policy_id: POLICY.into(),
            resource: "users".into(),
            object: "bae-1".into(),
            actor: Some(OWNER.into()),
        }
    );
    assert_eq!(effect_of(b"not a request"), Effect::Other);
}

//! Reading back what a signed request did.

use super::super::fixtures::*;
use super::*;
use crate::{PolicyAction, SubjectRef};

#[test]
fn registrations_and_relationship_changes_are_read_back_with_their_delegation() {
    let reader = SubjectRef::Actor(READER.into());
    assert_eq!(
        effect_of(&wire(0, "bae-1", register())),
        Effect::Owed(Lost::Registration {
            relationships: Vec::new(),
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
        Effect::Owed(Lost::Archive {
            policy_id: POLICY.into(),
            resource: "users".into(),
            object: "bae-1".into(),
            token: token(OWNER),
        })
    );
    assert_eq!(effect_of(b"not a request"), Effect::Other);
}

#[test]
fn a_voided_batch_keeps_every_archive_it_carried_and_counts_the_rest() {
    let batch = batch(
        3,
        [archive_calldata("bae-1"), archive_calldata("bae-2"), vec![]],
    );
    assert_eq!(
        effect_of(&batch),
        Effect::Batched(
            vec![
                Lost::Archive {
                    policy_id: POLICY.into(),
                    resource: "users".into(),
                    object: "bae-1".into(),
                    token: token(OWNER),
                },
                Lost::Archive {
                    policy_id: POLICY.into(),
                    resource: "users".into(),
                    object: "bae-2".into(),
                    token: token(OWNER),
                },
            ],
            1,
        )
    );
}

#[test]
fn a_registration_with_relationships_is_owed_again_with_them() {
    let derived = vec![acp::DerivedRelationship {
        relation: "author".into(),
        target_resource: "authors".into(),
        target_doc_id: "bae-2".into(),
    }];
    assert_eq!(
        effect_of(&wire(
            0,
            "bae-1",
            PolicyAction::RegisterObjectWithRelationships {
                relationships: &derived,
            }
        )),
        Effect::Owed(Lost::Registration {
            policy_id: POLICY.into(),
            resource: "users".into(),
            object: "bae-1".into(),
            relationships: derived.clone(),
            token: token(OWNER),
        })
    );
    let lost = Lost::Registration {
        policy_id: POLICY.into(),
        resource: "users".into(),
        object: "bae-1".into(),
        relationships: Vec::new(),
        token: token(OWNER),
    };
    let encoded = serde_json::to_string(&lost).unwrap();
    assert!(
        !encoded.contains("relationships"),
        "a registration without relationships keeps its encoding: {encoded}"
    );
}

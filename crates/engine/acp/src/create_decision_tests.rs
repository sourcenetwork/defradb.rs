use rapidhash::RapidHashSet;

use super::*;

fn link(relation: &str, resource: &str, object: &str) -> DerivedRelationship {
    DerivedRelationship {
        relation: relation.into(),
        target_resource: resource.into(),
        target_doc_id: object.into(),
    }
}

/// Decide `create` where the actor holds exactly the `granted` checks.
fn decide(create: &str, derived: &[DerivedRelationship], granted: &[(&str, &str, &str)]) -> bool {
    let granted: RapidHashSet<CreateCheck> = granted
        .iter()
        .map(|(computed, resource, object)| CreateCheck {
            computed: (*computed).into(),
            resource: (*resource).into(),
            object: (*object).into(),
        })
        .collect();
    workers::block_on(authorize_create(create, derived, |wanted| {
        let allowed = granted.contains(&wanted);
        async move { Ok(allowed) }
    }))
    .unwrap()
    .unwrap()
}

#[test]
fn an_arrow_holds_when_the_linked_object_grants_its_name() {
    let derived = [link("author", "author", "mike")];
    assert!(decide(
        "author->publish",
        &derived,
        &[("publish", "author", "mike")]
    ));
    assert!(!decide("author->publish", &derived, &[]));
    assert!(!decide(
        "author->publish",
        &derived,
        &[("publish", "author", "ana")]
    ));
}

#[test]
fn intersections_and_differences_combine_the_checks() {
    let derived = [
        link("author", "author", "mike"),
        link("imprint", "imprint", "ace"),
    ];
    let both = [("publish", "author", "mike"), ("publish", "imprint", "ace")];
    assert!(decide(
        "author->publish & imprint->publish",
        &derived,
        &both
    ));
    assert!(!decide(
        "author->publish & imprint->publish",
        &derived,
        &both[..1]
    ));
    assert!(decide(
        "author->publish - imprint->banned",
        &derived,
        &both[..1]
    ));
    assert!(!decide(
        "author->publish - imprint->banned",
        &derived,
        &[("publish", "author", "mike"), ("banned", "imprint", "ace")]
    ));
    assert!(decide(
        "author->publish + imprint->publish",
        &derived,
        &both[1..]
    ));
}

#[test]
fn an_arrow_through_a_relation_the_document_lacks_never_holds() {
    let checks = create_checks("author->publish", &[]).unwrap();
    assert!(checks.is_empty());
    assert!(!decide(
        "author->publish",
        &[],
        &[("publish", "author", "mike")]
    ));
}

#[test]
fn a_name_that_is_not_an_arrow_never_admits() {
    let derived = [link("author", "author", "mike")];
    assert!(!decide("author", &derived, &[("author", "author", "mike")]));
    assert!(!decide("_this", &derived, &[]));
}

#[test]
fn an_open_create_needs_no_check() {
    assert!(!is_gated(None));
    assert!(!is_gated(Some(" * ")));
    assert!(is_gated(Some("author->publish")));
    assert!(decide("*", &[], &[]));
}

#[test]
fn each_linked_object_is_checked_once() {
    let derived = [link("author", "author", "mike")];
    let checks = create_checks("author->publish & author->publish", &derived).unwrap();
    assert_eq!(
        checks,
        vec![CreateCheck {
            computed: "publish".into(),
            resource: "author".into(),
            object: "mike".into(),
        }]
    );
}

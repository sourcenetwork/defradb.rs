//! `Policy::management_sources`: the relations an owed revocation must touch
//! to take away the authority to change a relation on the same object.

use super::*;

fn computed(relation: &str) -> RelationExpression {
    RelationExpression::ComputedUserset {
        relation: relation.into(),
    }
}

fn policy(relations: Vec<Relation>) -> Policy {
    let resource = relations
        .into_iter()
        .fold(Resource::new("doc"), Resource::with_relation);
    Policy::new("p", "p").with_resource(resource)
}

fn sources(policy: &Policy) -> Vec<String> {
    policy.management_sources("doc", "reader")
}

#[test]
fn a_direct_manager_is_its_own_only_source() {
    let policy = policy(vec![
        Relation::direct("reader"),
        Relation::direct("manager").with_manages(vec!["reader"]),
        Relation::direct("writer"),
    ]);
    assert_eq!(sources(&policy), ["manager"]);
}

#[test]
fn every_manager_counts() {
    let policy = policy(vec![
        Relation::direct("reader"),
        Relation::direct("admin").with_manages(vec!["reader"]),
        Relation::direct("manager").with_manages(vec!["reader", "writer"]),
    ]);
    assert_eq!(sources(&policy), ["admin", "manager"]);
}

#[test]
fn a_computed_manager_reads_the_relation_it_is_computed_from() {
    let policy = policy(vec![
        Relation::direct("reader"),
        Relation::direct("admin"),
        Relation::computed("manager", computed("admin")).with_manages(vec!["reader"]),
    ]);
    assert_eq!(sources(&policy), ["admin", "manager"]);
}

#[test]
fn a_tuple_to_userset_reads_its_tuple_relation_on_this_object_only() {
    let policy = policy(vec![
        Relation::direct("reader"),
        Relation::direct("parent"),
        Relation::computed(
            "manager",
            RelationExpression::TupleToUserset {
                tuple_relation: "parent".into(),
                computed_relation: "owner".into(),
            },
        )
        .with_manages(vec!["reader"]),
    ]);
    assert_eq!(sources(&policy), ["manager", "parent"]);
}

#[test]
fn unions_intersections_and_differences_read_every_part() {
    let policy = policy(vec![
        Relation::direct("reader"),
        Relation::direct("editor"),
        Relation::direct("admin"),
        Relation::direct("staff"),
        Relation::direct("banned"),
        Relation::computed(
            "manager",
            RelationExpression::Difference {
                base: Box::new(RelationExpression::Union(vec![
                    computed("editor"),
                    RelationExpression::Intersection(vec![computed("admin"), computed("staff")]),
                ])),
                subtract: Box::new(computed("banned")),
            },
        )
        .with_manages(vec!["reader"]),
    ]);
    assert_eq!(
        sources(&policy),
        ["admin", "banned", "editor", "manager", "staff"]
    );
}

#[test]
fn a_cycle_ends_with_each_relation_once() {
    let policy = policy(vec![
        Relation::direct("reader"),
        Relation::computed(
            "manager",
            RelationExpression::Union(vec![RelationExpression::This, computed("deputy")]),
        )
        .with_manages(vec!["reader"]),
        Relation::computed("deputy", computed("manager")),
    ]);
    assert_eq!(sources(&policy), ["deputy", "manager"]);
}

#[test]
fn an_unknown_resource_or_an_unmanaged_relation_has_no_source() {
    let policy = policy(vec![
        Relation::direct("reader"),
        Relation::direct("manager").with_manages(vec!["writer"]),
    ]);
    assert!(sources(&policy).is_empty());
    assert!(policy.management_sources("other", "reader").is_empty());
}

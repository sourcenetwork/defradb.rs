use std::sync::Arc;

use zanzibar::{
    store::{MemoryZanzibarStore, ZanzibarStore},
    Did, PermissionEngine, Policy, Relation, RelationExpression, Relationship, Resource, Subject,
};

#[tokio::test]
async fn nested_entity_sets_follow_the_named_relation_and_revocation() {
    let store = Arc::new(MemoryZanzibarStore::new());
    let mut engine = PermissionEngine::new(store.clone());
    let policy = Policy::new("policy", "groups")
        .with_resource(Resource::new("document").with_relation(Relation::direct("reader")))
        .with_resource(
            Resource::new("group")
                .with_relation(Relation::direct("member"))
                .with_relation(Relation::direct("blocked"))
                .with_relation(Relation::computed(
                    "allowed",
                    RelationExpression::parse("member - blocked").unwrap(),
                )),
        );
    engine.add_policy(&policy);
    let actor = Did::new("did:key:reader").unwrap();
    let member = Relationship::with_entity("group", "inner", "member", actor.clone());
    let blocked = Relationship::with_entity("group", "inner", "blocked", actor.clone());
    for relationship in [
        Relationship::new(
            "document",
            "report",
            "reader",
            Subject::entity_set("group", "outer", "member"),
        ),
        Relationship::new(
            "group",
            "outer",
            "member",
            Subject::entity_set("group", "inner", "allowed"),
        ),
        member.clone(),
    ] {
        store
            .store_relationship("policy", &relationship)
            .await
            .unwrap();
    }
    let check = async |expected| {
        assert_eq!(
            engine
                .check("policy", "document", "report", "reader", &actor)
                .await
                .unwrap(),
            expected
        );
        assert_eq!(
            engine
                .check_blocking("policy", "document", "report", "reader", &actor)
                .unwrap(),
            expected
        );
        let explanation = engine
            .explain("policy", "document", "report", "reader", &actor)
            .await
            .unwrap();
        assert_eq!(explanation.granted, expected);
    };
    check(true).await;
    store.store_relationship("policy", &blocked).await.unwrap();
    check(false).await;
    store.delete_relationship("policy", &blocked).await.unwrap();
    check(true).await;
    store.delete_relationship("policy", &member).await.unwrap();
    check(false).await;
}

#[tokio::test]
async fn entity_set_cycles_do_not_hide_a_reachable_grant() {
    let store = Arc::new(MemoryZanzibarStore::new());
    let mut engine = PermissionEngine::new(store.clone());
    engine.add_policy(
        &Policy::new("policy", "cycles")
            .with_resource(Resource::new("group").with_relation(Relation::direct("member"))),
    );
    let actor = Did::new("did:key:reader").unwrap();
    for (source, target) in [("a", "b"), ("b", "a"), ("b", "c")] {
        store
            .store_relationship(
                "policy",
                &Relationship::new(
                    "group",
                    source,
                    "member",
                    Subject::entity_set("group", target, "member"),
                ),
            )
            .await
            .unwrap();
    }
    assert!(!engine
        .check("policy", "group", "a", "member", &actor)
        .await
        .unwrap());
    let grant = Relationship::with_entity("group", "c", "member", actor.clone());
    store.store_relationship("policy", &grant).await.unwrap();
    assert!(engine
        .check("policy", "group", "a", "member", &actor)
        .await
        .unwrap());
    assert!(
        engine
            .explain("policy", "group", "a", "member", &actor)
            .await
            .unwrap()
            .granted
    );
    store.delete_relationship("policy", &grant).await.unwrap();
    assert!(!engine
        .check("policy", "group", "a", "member", &actor)
        .await
        .unwrap());
    assert!(
        !engine
            .explain("policy", "group", "a", "member", &actor)
            .await
            .unwrap()
            .granted
    );
}

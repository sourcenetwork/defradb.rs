use std::sync::Arc;

use zanzibar::{
    store::{MemoryZanzibarStore, ZanzibarStore},
    Did, PermissionCheckRequest, PermissionEngine, Policy, Relation, RelationExpression,
    Relationship, Resource,
};

#[tokio::test]
async fn an_exclusion_cycle_cannot_authorize_the_actor_it_excludes() {
    let store = Arc::new(MemoryZanzibarStore::new());
    let mut engine = PermissionEngine::new(store.clone());
    let policy = Policy::new("policy", "cyclic exclusion").with_resource(
        Resource::new("document")
            .with_relation(Relation::direct("reader"))
            .with_relation(Relation::computed(
                "read",
                RelationExpression::parse("reader - excluded").unwrap(),
            ))
            .with_relation(Relation::computed(
                "excluded",
                RelationExpression::parse("read").unwrap(),
            )),
    );
    policy.validate().unwrap();
    engine.add_policy(&policy);
    let actor = Did::new("did:key:reader").unwrap();
    store
        .store_relationship(
            "policy",
            &Relationship::with_entity("document", "report", "reader", actor.clone()),
        )
        .await
        .unwrap();
    assert!(engine
        .check("policy", "document", "report", "read", &actor)
        .await
        .is_err());
    assert!(engine
        .check_blocking("policy", "document", "report", "read", &actor)
        .is_err());
    assert!(engine
        .explain("policy", "document", "report", "read", &actor)
        .await
        .is_err());
    let requests = [PermissionCheckRequest::new(
        "policy", "document", "report", "read", &actor,
    )];
    assert!(engine.check_many(&requests).await[0].is_err());
}

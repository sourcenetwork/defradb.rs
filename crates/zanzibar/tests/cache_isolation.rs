use std::sync::Arc;
use zanzibar::{
    store::{MemoryZanzibarStore, ZanzibarStore},
    Did, PermissionCheckRequest, PermissionEngine, Policy, Relation, Relationship, Resource,
};

#[tokio::test]
async fn batched_checks_keep_policy_authority_separate_in_both_orders() {
    let store = Arc::new(MemoryZanzibarStore::new());
    let mut engine = PermissionEngine::new(store.clone());
    for policy in ["allowed", "denied"] {
        engine
            .add_policy(&Policy::new(policy, policy).with_resource(
                Resource::new("document").with_relation(Relation::direct("reader")),
            ));
    }
    let actor = Did::new("did:key:reader").unwrap();
    store
        .store_relationship(
            "allowed",
            &Relationship::with_entity("document", "report", "reader", actor.clone()),
        )
        .await
        .unwrap();
    for policies in [["allowed", "denied"], ["denied", "allowed"]] {
        let requests: Vec<_> = policies
            .iter()
            .map(|policy| {
                PermissionCheckRequest::new(policy, "document", "report", "reader", &actor)
            })
            .collect();
        let results = engine.check_many(&requests).await;
        for (policy, result) in policies.into_iter().zip(results) {
            assert_eq!(result.unwrap(), policy == "allowed");
        }
    }
}

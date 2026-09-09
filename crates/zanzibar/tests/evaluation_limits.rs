use std::sync::Arc;
use zanzibar::{
    engine::{MAX_EVALUATION_DEPTH, MAX_EVALUATION_STEPS},
    error::Error,
    store::{MemoryZanzibarStore, ZanzibarStore},
    Did, PermissionEngine, Policy, Relation, RelationExpression, Relationship, Resource, Subject,
};

#[tokio::test]
async fn deep_entity_sets_fail_in_checks_and_explanations() {
    let store = Arc::new(MemoryZanzibarStore::new());
    let mut engine = PermissionEngine::new(store.clone());
    engine.add_policy(
        &Policy::new("p", "p")
            .with_resource(Resource::new("group").with_relation(Relation::direct("member"))),
    );
    let actor = Did::new("did:key:actor").unwrap();
    for index in 0..MAX_EVALUATION_DEPTH {
        store
            .store_relationship(
                "p",
                &Relationship::new(
                    "group",
                    index.to_string(),
                    "member",
                    Subject::entity_set("group", (index + 1).to_string(), "member"),
                ),
            )
            .await
            .unwrap();
    }
    store
        .store_relationship(
            "p",
            &Relationship::with_entity(
                "group",
                MAX_EVALUATION_DEPTH.to_string(),
                "member",
                actor.clone(),
            ),
        )
        .await
        .unwrap();
    assert!(matches!(
        engine.check("p", "group", "0", "member", &actor).await,
        Err(Error::EvaluationLimitExceeded("depth"))
    ));
    assert!(matches!(
        engine.explain("p", "group", "0", "member", &actor).await,
        Err(Error::EvaluationLimitExceeded("depth"))
    ));
    assert!(matches!(
        engine.check_blocking("p", "group", "0", "member", &actor),
        Err(Error::EvaluationLimitExceeded("depth"))
    ));
    assert!(engine
        .check("p", "group", "1", "member", &actor)
        .await
        .unwrap());
}

#[tokio::test]
async fn wide_expression_work_is_bounded_without_becoming_a_denial() {
    let store = Arc::new(MemoryZanzibarStore::new());
    let mut engine = PermissionEngine::new(store);
    engine.add_policy(
        &Policy::new("p", "p").with_resource(Resource::new("group").with_relation(
            Relation::computed(
                "member",
                RelationExpression::Union(vec![RelationExpression::This; MAX_EVALUATION_STEPS]),
            ),
        )),
    );
    let actor = Did::new("did:key:actor").unwrap();
    assert!(matches!(
        engine.check("p", "group", "0", "member", &actor).await,
        Err(Error::EvaluationLimitExceeded("steps"))
    ));
    assert!(matches!(
        engine.explain("p", "group", "0", "member", &actor).await,
        Err(Error::EvaluationLimitExceeded("steps"))
    ));
}

#[tokio::test]
async fn batches_share_work_limits_including_cached_checks() {
    let mut engine = PermissionEngine::new(Arc::new(MemoryZanzibarStore::new()));
    engine.add_policy(
        &Policy::new("p", "p")
            .with_resource(Resource::new("group").with_relation(Relation::direct("member"))),
    );
    let actor = Did::new("did:key:actor").unwrap();
    let requests = vec![
        zanzibar::PermissionCheckRequest::new("p", "group", "0", "member", &actor);
        MAX_EVALUATION_STEPS + 1
    ];
    let results = engine.check_many(&requests).await;
    assert!(results[..MAX_EVALUATION_STEPS]
        .iter()
        .all(|result| matches!(result, Ok(false))));
    assert!(matches!(
        results.last().unwrap(),
        Err(Error::EvaluationLimitExceeded("steps"))
    ));
    assert!(!engine
        .check("p", "group", "0", "member", &actor)
        .await
        .unwrap());
}

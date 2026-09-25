use std::sync::Arc;

use acp::{
    policy_yaml::{build_policy, parse_policy_yaml, validate_policy_expressions},
    MemoryZanzibarStore, PermissionEngine, Policy, Relationship, Subject, ZanzibarStore,
};
use identity::Did;

const POLICY: &str = "name: files\ndescription: documents\nmeta:\n  description: user metadata\nactor:\n  relations:\n    - name: member\n      doc: delegated members\n      types: [actor]\nresources:\n  - name: file\n    description: stored files\n    relations:\n      - name: reader\n        doc: reader grants\n        types: [actor, actor->member]\n    permissions:\n      - name: read\n        doc: read files\n        expr: reader\n";

#[tokio::test]
async fn metadata_and_actor_subjects_survive_policy_roundtrip() {
    let built = build_policy(&parse_policy_yaml(POLICY).unwrap(), 1).unwrap();
    let policy: Policy = serde_json::from_slice(&serde_json::to_vec(&built).unwrap()).unwrap();
    assert_eq!(policy.description, "documents");
    assert_eq!(policy.attributes["description"], "user metadata");
    assert_eq!(
        policy.get_resource("file").unwrap().description,
        "stored files"
    );
    assert_eq!(
        policy.get_relation("file", "read").unwrap().description,
        "read files"
    );
    assert_eq!(
        policy.get_relation("actor", "member").unwrap().description,
        "delegated members"
    );
    let alice = Did::new("did:key:alice").unwrap();
    let bob = Did::new("did:key:bob").unwrap();
    let store = Arc::new(MemoryZanzibarStore::new());
    let roles = [
        Relationship::with_entity("actor", bob.as_str(), "member", alice.clone()),
        Relationship::new(
            "file",
            "report",
            "reader",
            Subject::entity_set("actor", bob.as_str(), "member"),
        ),
        Relationship::new(
            "file",
            "direct",
            "reader",
            Subject::entity_set("actor", bob.as_str(), ""),
        ),
    ];
    for relationship in roles {
        relationship.validate(&policy).unwrap();
        store
            .store_relationship(&policy.id, &relationship)
            .await
            .unwrap();
    }
    let mut engine = PermissionEngine::new(store);
    engine.add_policy(&policy);
    assert!(engine
        .check(&policy.id, "file", "report", "read", &alice)
        .await
        .unwrap());
    assert!(!engine
        .check(&policy.id, "file", "report", "read", &bob)
        .await
        .unwrap());
    assert!(engine
        .check(&policy.id, "file", "direct", "read", &bob)
        .await
        .unwrap());
    assert!(!engine
        .check(&policy.id, "file", "direct", "read", &alice)
        .await
        .unwrap());
}

#[test]
fn rejects_unknown_duplicate_and_ambiguous_schema() {
    for definition in [
        POLICY.replace("description: documents", "descripton: documents"),
        POLICY.replace("expr: reader", "expression: reader"),
        POLICY.replace(
            "description: user metadata",
            "description: first\n  description: second",
        ),
        POLICY.replace("name: files", "name: ''"),
        POLICY.replace("name: file\n", "name: actor\n"),
        POLICY.replace("name: read\n", "name: owner\n"),
        POLICY.replace("actor->member", "actor->missing"),
        POLICY.replace("name: reader\n", "name: 'reader/bad'\n"),
        POLICY.replace(
            "types: [actor]\n",
            "types: [actor]\n      manages: [missing]\n",
        ),
    ] {
        assert!(
            parse_policy_yaml(&definition)
                .map_err(|error| error.to_string())
                .and_then(|parsed| build_policy(&parsed, 1).map_err(|error| error.to_string()))
                .is_err(),
            "accepted {definition}"
        );
    }
}

#[test]
fn cross_object_owner_is_a_valid_permission_target() {
    let parsed = parse_policy_yaml("name: files\nresources:\n  - name: file\n    relations:\n      - name: parent\n        types: [file]\n    permissions:\n      - name: read\n        expr: parent->owner\n").unwrap();
    validate_policy_expressions(&parsed).unwrap();
    build_policy(&parsed, 1).unwrap();
}

#[test]
fn malformed_expressions_return_errors_without_panicking() {
    for expression in [
        "éé + reader".to_string(),
        format!("{}reader{}", "(".repeat(129), ")".repeat(129)),
    ] {
        let definition = POLICY.replace("expr: reader", &format!("expr: {expression}"));
        assert!(build_policy(&parse_policy_yaml(&definition).unwrap(), 1).is_err());
    }
}

#[test]
fn metadata_serialization_has_a_stable_key_order() {
    let policy = build_policy(
        &parse_policy_yaml("name: files\nmeta:\n  z: last\n  a: first\n").unwrap(),
        1,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_string(&policy.attributes).unwrap(),
        r#"{"a":"first","z":"last"}"#
    );
}

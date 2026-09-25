use std::sync::Arc;

use acp::{
    policy_yaml::{build_policy, parse_policy_yaml},
    MemoryZanzibarStore, PermissionEngine, Policy, Relationship, ZanzibarStore,
};
use identity::Did;

const DEFINITION: &str = "name: files\nresources:\n  - name: file\n    relations:\n      - name: reader\n      - name: writer\n    permissions:\n      - name: read\n        expr: reader - writer\n      - name: write\n        expr: writer\n";

#[test]
fn defra_requires_read_and_write_permissions() {
    for permission in ["read", "write"] {
        let definition = format!(
            "spec: defra\nname: files\nresources:\n  - name: file\n    permissions:\n      - name: {permission}\n"
        );
        assert!(build_policy(&parse_policy_yaml(&definition).unwrap(), 1).is_err());
        assert!(build_policy(
            &parse_policy_yaml(&definition.replace("spec: defra\n", "")).unwrap(),
            1
        )
        .is_ok());
    }
}

#[tokio::test]
async fn defra_write_grants_read_after_record_roundtrip() {
    let writer = Did::new("did:key:writer").unwrap();
    let stranger = Did::new("did:key:stranger").unwrap();
    for spec in ["none", "defra", "DeFrA"] {
        let definition = format!("spec: {spec}\n{DEFINITION}");
        let built = build_policy(&parse_policy_yaml(&definition).unwrap(), 1).unwrap();
        let encoded = serde_json::to_value(&built).unwrap();
        if spec == "none" {
            assert!(encoded.get("specification").is_none());
        } else {
            assert_eq!(encoded["specification"], "defra");
        }
        let policy: Policy = serde_json::from_value(encoded).unwrap();
        let store = Arc::new(MemoryZanzibarStore::new());
        store
            .store_relationship(
                &policy.id,
                &Relationship::with_entity("file", "report", "writer", writer.clone()),
            )
            .await
            .unwrap();
        let mut engine = PermissionEngine::new(store);
        engine.add_policy(&policy);
        assert!(engine
            .check(&policy.id, "file", "report", "write", &writer)
            .await
            .unwrap());
        assert_eq!(
            engine
                .check(&policy.id, "file", "report", "read", &writer)
                .await
                .unwrap(),
            spec != "none"
        );
        assert!(!engine
            .check(&policy.id, "file", "report", "read", &stranger)
            .await
            .unwrap());
    }
}

#[test]
fn unknown_specification_is_rejected() {
    assert!(parse_policy_yaml(&format!("spec: defar\n{DEFINITION}")).is_err());
}

use std::sync::Arc;

use acp::policy_yaml::{build_policy, parse_policy_yaml};
use acp::{DocumentACP, PersistentZanzibarStore, ZanzibarDocumentACP};
use identity::Did;
use storage::RegolithStore;
use zanzibar::{Subject, ZanzibarStore};

#[test]
fn parent_relation_requires_an_explicit_grant() {
    workers::block_on(async {
        let store = Arc::new(PersistentZanzibarStore::from_store(Arc::new(RegolithStore::in_memory().unwrap())));
        let yaml = "name: gates\nresources:\n- name: node\n  relations: [{name: publisher, types: [actor]}]\n- name: job\n  relations: [{name: parent, types: [node]}]\n  create: {parent: parent, requires: publisher}\n";
        let policy = build_policy(&parse_policy_yaml(yaml).unwrap(), 1).unwrap();
        store.store_policy(&policy).await.unwrap();
        let acp = ZanzibarDocumentACP::new(store);
        let owner = Did::new("did:key:owner").unwrap();
        let publisher = Did::new("did:key:publisher").unwrap();
        acp.register_doc_object(&owner, &policy.id, "node", "a").await.unwrap();
        let parent = acp.registration_parent(&policy.id, "job", "a").await.unwrap();
        assert!(acp.register_under_parent(&owner, &policy.id, "job", "b", &parent).await.is_err());
        assert!(acp.register_under_parent(&publisher, &policy.id, "job", "b", &parent).await.is_err());
        acp.add_relationship(&owner, Subject::entity(publisher.clone()), &policy.id, "node", "a", "publisher", &[]).await.unwrap();
        acp.register_under_parent(&publisher, &policy.id, "job", "b", &parent).await.unwrap();
    }).unwrap();
}

#[test]
fn local_registration_checks_parent_before_writing_child() {
    workers::block_on(async {
    let store = Arc::new(PersistentZanzibarStore::from_store(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));
    let definition = parse_policy_yaml("name: gates\nresources:\n- name: node\n- name: job\n  relations: [{name: parent, types: [node]}]\n  create: {parent: parent, requires: owner}\n").unwrap();
    let policy = build_policy(&definition, 1).unwrap();
    store.store_policy(&policy).await.unwrap();
    let acp = ZanzibarDocumentACP::new(store.clone());
    let owner = Did::new("did:key:owner").unwrap();
    let other = Did::new("did:key:other").unwrap();
    let id = &policy.id;
    acp.register_doc_object(&owner, id, "node", "a")
        .await
        .unwrap();
    let parent = acp.registration_parent(id, "job", "a").await.unwrap();
    assert!(acp
        .register_doc_object(&owner, id, "job", "b")
        .await
        .is_err());
    assert!(acp
        .register_under_parent(&other, id, "job", "b", &parent)
        .await
        .is_err());
    assert!(!acp.is_doc_registered(id, "job", "b").await.unwrap());
    acp.register_under_parent(&owner, id, "job", "b", &parent)
        .await
        .unwrap();
    assert_eq!(
        acp.get_doc_owner(id, "job", "b").await.unwrap(),
        Some(owner.clone())
    );
    assert!(store
        .has_relationship(
            id,
            "job",
            "b",
            "parent",
            &Subject::entity_set("node", "a", "")
        )
        .await
        .unwrap());
    acp.register_doc_object(&other, id, "node", "c")
        .await
        .unwrap();
    assert!(acp
        .add_relationship(
            &owner,
            Subject::entity_set("node", "c", ""),
            id,
            "job",
            "b",
            "parent",
            &[]
        )
        .await
        .is_err());
    }).unwrap();
}

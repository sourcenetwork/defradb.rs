//! A relation typed `actor:individual` admits one actor at a time, never the
//! all-actors wildcard, and changes the policy's ID.

use std::sync::Arc;

use acp::policy_yaml::{build_policy, parse_policy_yaml};
use acp::{DocumentACP, PersistentZanzibarStore, ZanzibarDocumentACP};
use identity::Did;
use storage::RegolithStore;
use zanzibar::{Subject, ZanzibarStore};

#[test]
fn individual_publisher_rejects_wildcard() {
    workers::block_on(async {
        let store = Arc::new(PersistentZanzibarStore::from_store(Arc::new(
            RegolithStore::in_memory().unwrap(),
        )));
        let yaml = "name: gates\nresources:\n- name: node\n  relations: [{name: publisher, types: [actor:individual]}]\n";
        let policy = build_policy(&parse_policy_yaml(yaml).unwrap(), 1).unwrap();
        let legacy = build_policy(
            &parse_policy_yaml(&yaml.replace("actor:individual", "actor")).unwrap(),
            1,
        )
        .unwrap();
        assert_ne!(policy.id, legacy.id);
        store.store_policy(&legacy).await.unwrap();
        store.store_policy(&policy).await.unwrap();
        let acp = ZanzibarDocumentACP::new(store);
        let owner = Did::new("did:key:owner").unwrap();
        let publisher = Did::new("did:key:publisher").unwrap();
        acp.register_doc_object(&owner, &legacy.id, "node", "legacy", &[])
            .await
            .unwrap();
        acp.add_relationship(&owner, Subject::Wildcard, &legacy.id, "node", "legacy", "publisher", &[])
            .await
            .unwrap();
        acp.register_doc_object(&owner, &policy.id, "node", "a", &[])
            .await
            .unwrap();
        assert!(acp
            .add_relationship(&owner, Subject::Wildcard, &policy.id, "node", "a", "publisher", &[])
            .await
            .is_err());
        acp.add_relationship(
            &owner,
            Subject::entity(publisher.clone()),
            &policy.id,
            "node",
            "a",
            "publisher",
            &[],
        )
        .await
        .unwrap();
    })
    .unwrap();
}

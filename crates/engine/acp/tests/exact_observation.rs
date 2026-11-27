//! `observe_actor_relationship` on the local ACP, against
//! `proofs/lean/ExactRelationshipObservation.lean`: only a caller who may
//! manage the relation gets an answer, the answer is the exact tuple, and an
//! unauthorized caller gets one refusal whatever the document's state.

use std::sync::Arc;

use acp::{
    policy_yaml::{build_policy, parse_policy_yaml},
    ActorRelationshipObservation, DocumentACP, Error, MemoryZanzibarStore, ZanzibarDocumentACP,
    ZanzibarStore,
};
use identity::Did;

const POLICY: &str = r#"
name: exact observation
resources:
- name: records
  permissions:
  - name: read
    expr: reader
  - name: update
    expr: reader
  - name: delete
    expr: reader
  relations:
  - name: reader
    types: [actor]
  - name: reader_manager
    types: [actor]
    manages: [reader]
"#;

fn did(name: &str) -> Did {
    Did::new(format!("did:key:{name}")).unwrap()
}

async fn acp() -> (ZanzibarDocumentACP<MemoryZanzibarStore>, String) {
    let policy = build_policy(&parse_policy_yaml(POLICY).unwrap(), 1).unwrap();
    let store = Arc::new(MemoryZanzibarStore::new());
    store.store_policy(&policy).await.unwrap();
    let acp = ZanzibarDocumentACP::new(store);
    acp.register_doc_object(&did("owner"), &policy.id, "records", "doc", &[])
        .await
        .unwrap();
    (acp, policy.id)
}

async fn observe(
    acp: &ZanzibarDocumentACP<MemoryZanzibarStore>,
    policy: &str,
    requester: &Did,
    target: &Did,
    document: &str,
) -> acp::Result<bool> {
    acp.observe_actor_relationship(ActorRelationshipObservation {
        requester,
        target,
        policy,
        resource: "records",
        document,
        relation: "reader",
    })
    .await
}

#[test]
fn owners_and_managers_observe_the_exact_tuple() {
    workers::block_on(async {
        let (acp, policy) = acp().await;
        let (owner, manager, reader) = (did("owner"), did("manager"), did("reader"));
        acp.add_actor_relationship(&owner, &reader, &policy, "records", "doc", "reader", &[])
            .await
            .unwrap();
        acp.add_actor_relationship(
            &owner,
            &manager,
            &policy,
            "records",
            "doc",
            "reader_manager",
            &[],
        )
        .await
        .unwrap();

        assert!(observe(&acp, &policy, &owner, &reader, "doc")
            .await
            .unwrap());
        assert!(observe(&acp, &policy, &manager, &reader, "doc")
            .await
            .unwrap());
        assert!(!observe(&acp, &policy, &owner, &manager, "doc")
            .await
            .unwrap());
    })
    .unwrap()
}

#[test]
fn an_unauthorized_caller_learns_nothing_about_the_document() {
    workers::block_on(async {
        let (acp, policy) = acp().await;
        let (owner, reader) = (did("owner"), did("reader"));
        acp.add_actor_relationship(&owner, &reader, &policy, "records", "doc", "reader", &[])
            .await
            .unwrap();

        let registered = observe(&acp, &policy, &reader, &reader, "doc").await;
        let unregistered = observe(&acp, &policy, &reader, &reader, "absent").await;
        let elsewhere = observe(&acp, &policy, &owner, &reader, "absent").await;

        for refusal in [registered, unregistered, elsewhere] {
            assert!(
                matches!(&refusal, Err(Error::NotManager { operation }) if operation == "observe relationship"),
                "{refusal:?}"
            );
        }
    })
    .unwrap()
}

#[test]
fn a_wildcard_is_not_an_exact_actor() {
    workers::block_on(async {
        let (acp, policy) = acp().await;
        let owner = did("owner");

        for (requester, target) in [(Did::wildcard(), owner.clone()), (owner, Did::wildcard())] {
            assert!(matches!(
                observe(&acp, &policy, &requester, &target, "doc").await,
                Err(Error::PermissionDenied(_))
            ));
        }
    })
    .unwrap()
}

//! Giving a document to another owner on a node's own Vera deployment: the
//! creator's grant and the transfer are owed in the create's transaction,
//! reach the chain in that order after the commit, and the chain then names
//! the new owner, which manages the document while its creator no longer
//! does.

use std::time::Duration;

use acp::{DocumentACP as _, RemotePolicies as _};
use vera::vera_rs::{caller_delegation, DelegationSigner as _, Ed25519Signer};

use super::{
    chain_reads, create_user, created, node, reads_doc, users, RelationshipChange, POLICY, STRANGER,
};

const WAIT: Duration = Duration::from_secs(60);
const LATER: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";

/// How many commits of `doc` `reader` reads in its commit history.
async fn commits(node: &defra_node::Node, reader: &identity::Did, doc: &str) -> usize {
    let response = node
        .execute_request(
            defra_node::QueryRequest::new(format!(r#"{{ _commits(docID: "{doc}") {{ cid }} }}"#))
                .with_identity(Some(reader.clone())),
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("data")["_commits"]
        .as_array()
        .map_or(0, Vec::len)
}

/// A caller whose key the node does not hold, acting under its delegation.
fn new_owner() -> (identity::Did, Ed25519Signer) {
    let key = identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
    let signer = Ed25519Signer::from_slice(&key.private_key_bytes()).expect("key");
    (identity::Did::new(signer.did()).unwrap(), signer)
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_transferred_document_is_its_new_owners_on_the_chain_after_its_creators_grant() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let creator = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let reader = identity::Did::new(STRANGER).unwrap();
        let (owner, signer) = new_owner();
        let policy = vera
            .add_policy(creator.as_str(), POLICY, None)
            .await
            .expect("policy");
        node.add_collection(&users(&policy))
            .await
            .expect("collection");

        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        let doc = created(
            transaction
                .execute_request(create_user("agent", &creator))
                .await,
        );
        let granted = transaction
            .acting_for(creator.clone())
            .change_dac_actor_relationship("Users", &doc, "reader", reader.as_str(), false)
            .await
            .expect("staged grant");
        assert!(matches!(granted, RelationshipChange::Staged { .. }));
        transaction
            .acting_for(creator.clone())
            .transfer_dac_object_owner("Users", &doc, owner.as_str())
            .await
            .expect("staged transfer");
        transaction.commit().await.expect("commit");
        let wait = node
            .with_identity(creator.clone())
            .await_owed_acp(handle, WAIT)
            .await
            .expect("owed wait");
        assert!(matches!(wait, acp::OwedWait::Settled), "{wait:?}");

        let named = vera
            .get_doc_owner(&policy, "users", &doc)
            .await
            .expect("owner")
            .expect("registered");
        assert_eq!(named, owner);
        assert!(chain_reads(&node, &reader, &policy, &doc).await);
        assert!(chain_reads(&node, &owner, &policy, &doc).await);
        assert!(!chain_reads(&node, &creator, &policy, &doc).await);
        assert!(reads_doc(&node, &owner, &policy, &doc).await);
        assert!(!reads_doc(&node, &creator, &policy, &doc).await);
        assert!(commits(&node, &owner, &doc).await > 0);
        assert_eq!(commits(&node, &creator, &doc).await, 0);

        // The creator manages nothing on it any more; the new owner does,
        // under its own delegation, and a reader may not give it away.
        let later = identity::Did::new(LATER).unwrap();
        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        let refused = transaction
            .acting_for(creator.clone())
            .change_dac_actor_relationship("Users", &doc, "reader", later.as_str(), false)
            .await;
        assert!(
            matches!(
                refused,
                Err(defra_node::Error::Acp(acp::Error::NotOwner { .. }))
            ),
            "{refused:?}"
        );
        let stolen = transaction
            .acting_for(reader.clone())
            .transfer_dac_object_owner("Users", &doc, reader.as_str())
            .await;
        assert!(
            matches!(
                stolen,
                Err(defra_node::Error::Acp(acp::Error::NotOwner { .. }))
            ),
            "{stolen:?}"
        );
        let audience = vera
            .light_client_status()
            .expect("status")
            .delegation
            .expect("a vera.rs node names its delegation audience");
        let delegation = caller_delegation(&signer, &audience, Duration::from_secs(120))
            .await
            .expect("delegation");
        let staged = transaction
            .acting_for(owner.clone())
            .with_bearer(delegation)
            .change_dac_actor_relationship("Users", &doc, "reader", later.as_str(), false)
            .await
            .expect("the new owner's grant");
        assert!(
            matches!(
                staged,
                RelationshipChange::Staged {
                    authority_checked: true,
                    ..
                }
            ),
            "{staged:?}"
        );
        transaction.commit().await.expect("commit");
        let wait = node
            .with_identity(owner.clone())
            .await_owed_acp(handle, WAIT)
            .await
            .expect("owed wait");
        assert!(matches!(wait, acp::OwedWait::Settled), "{wait:?}");
        assert!(chain_reads(&node, &later, &policy, &doc).await);
        drop(vera);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

/// Whether the Vera node lets `actor` read `doc` of `resource`.
async fn reads_resource(
    node: &defra_node::Node,
    actor: &identity::Did,
    policy: &str,
    resource: &str,
    doc: &str,
) -> bool {
    node.vera_acp()
        .expect("vera.rs document ACP")
        .check_doc_access(
            &acp::Identity::Authenticated(actor.clone()),
            acp::DocumentPermission::Read,
            policy,
            resource,
            doc,
        )
        .await
        .expect("access decision")
}

/// A document of a governed collection, created in a transaction with the
/// relationship its bound field carries and given away in the same one:
/// it registers with that relationship, the transfer moves its ownership,
/// and the derived relationship stays, so a reader of the parent still
/// reads it.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_transferred_gated_document_keeps_its_derived_relationship() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let creator = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let (owner, _) = new_owner();
        let reader = identity::Did::new(STRANGER).unwrap();
        let jobs = r#"
  - name: jobs
    permissions:
      - name: read
        expr: parent->read
      - name: update
        expr: parent->read
      - name: delete
        expr: parent->read
    relations:
      - name: parent
        types:
          - users
    collection:
      create: parent->read
"#;
        let policy = vera
            .add_policy(creator.as_str(), &format!("{POLICY}{jobs}"), None)
            .await
            .expect("policy");
        let children = format!(
            r#"type Job @governed(root: "jobs") @policy(id: "{policy}", resource: "jobs") {{ parent: Users @immutable @policy(relation: "parent") }}"#
        );
        node.add_collection(&format!("{}\n{children}", users(&policy)))
            .await
            .expect("collections");
        let parent = created(node.execute_request(create_user("parent", &creator)).await);

        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        let response = transaction
            .execute_request(
                defra_node::QueryRequest::new(format!(
                    r#"mutation {{ add_Job(input: {{parent: "{parent}"}}) {{ _docID }} }}"#
                ))
                .with_identity(Some(creator.clone())),
            )
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let child = response.data.expect("data")["add_Job"][0]["_docID"]
            .as_str()
            .expect("document ID")
            .to_owned();
        transaction
            .acting_for(creator.clone())
            .transfer_dac_object_owner("Job", &child, owner.as_str())
            .await
            .expect("staged transfer");
        transaction.commit().await.expect("commit");
        let wait = node
            .with_identity(creator.clone())
            .await_owed_acp(handle, WAIT)
            .await
            .expect("owed wait");
        assert!(matches!(wait, acp::OwedWait::Settled), "{wait:?}");

        assert_eq!(
            vera.get_doc_owner(&policy, "jobs", &child)
                .await
                .expect("owner"),
            Some(owner.clone())
        );
        assert!(!reads_resource(&node, &reader, &policy, "jobs", &child).await);
        vera.add_actor_relationship(&creator, &reader, &policy, "users", &parent, "reader", &[])
            .await
            .expect("grant on the parent");
        assert!(
            reads_resource(&node, &reader, &policy, "jobs", &child).await,
            "the parent relationship stays with the transferred document"
        );
        drop(vera);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}

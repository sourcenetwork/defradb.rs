//! The documents one transaction creates are registered on the Vera node
//! together: one block for up to a full list of them, where each used to take
//! a block of its own.

use acp::RemotePolicies as _;
use std::time::{Duration, Instant};

use super::{create_user, created, node, reads_doc, users, POLICY, STRANGER};

#[test]
fn certified_sentinel_checks_owner_parent_and_device_at_one_root() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().unwrap();
        let owner = identity::Did::new(vera.self_did().unwrap()).unwrap();
        let definition = "name: sentinel\nresources:\n- name: nodes\n  relations: [{name: transport_publisher, types: [actor]}]\n- name: device_groups\n  relations: [{name: member, types: [actor]}]\n- name: jobs\n  relations: [{name: parent, types: [nodes]}]\n  permissions: [{name: read}, {name: update}, {name: delete}]\n  collection: {create: parent->transport_publisher}\n";
        let policy = vera.add_policy(owner.as_str(), definition, None).await.unwrap();
        let sdl = format!(r#"type SharedJob @policy(id: "{policy}", resource: "jobs") @branchable {{ parent_id: String! @immutable }}"#);
        let child = query::parse_sdl(&sdl).unwrap()[0].collection_id.clone();
        let acp = node.document_acp();
        acp.register_object(&owner, &policy, "nodes", "node").await.unwrap();
        acp.register_object(&owner, &policy, "device_groups", "group").await.unwrap();
        acp.add_relationship(&owner, acp::Subject::entity(owner.clone()), &policy, "device_groups", "group", "member", &[]).await.unwrap();
        acp.add_relationship(&owner, acp::Subject::entity(owner.clone()), &policy, "nodes", "node", "transport_publisher", &[]).await.unwrap();
        let parent = [acp::DerivedRelationship {
            relation: "parent".into(),
            target_resource: "nodes".into(),
            target_doc_id: "node".into(),
        }];
        acp.register_doc_object(&owner, &policy, "jobs", &child, &parent).await.unwrap();
        let mut query = acp::TransportSentinelQuery {
            policy, resource: "jobs".into(), child, parent: "node".into(),
            device_node: "node".into(), group: "group".into(), account: owner.to_string(), device: owner.to_string(),
        };
        assert!(vera.verify_transport_sentinel(&query).await.unwrap());
        query.device = STRANGER.into();
        assert!(!vera.verify_transport_sentinel(&query).await.unwrap());
        let follower = identity::Did::new(STRANGER).unwrap();
        acp.add_relationship(&owner, acp::Subject::entity(follower.clone()), &query.policy, "nodes", "node", "transport_publisher", &[]).await.unwrap();
        acp.add_relationship(&owner, acp::Subject::entity(follower.clone()), &query.policy, "device_groups", "group", "member", &[]).await.unwrap();
        assert!(vera.verify_transport_sentinel(&query).await.unwrap());
        let context = defra_core::RequestContext::default().with_identity(Some(follower.to_string()));
        let mut wrong_child = query.clone();
        wrong_child.child = "different".into();
        assert!(node.restore_shared_transport_collection_as(&context, &sdl, wrong_child).await.is_err());
        assert!(node.get_collection("SharedJob").unwrap().is_none());
        node.restore_shared_transport_collection_as(&context, &sdl, query.clone()).await.unwrap();
        query.parent = "wrong".into();
        assert!(!matches!(vera.verify_transport_sentinel(&query).await, Ok(true)));
        query.parent = "node".into();
        acp.add_relationship(&owner, acp::Subject::Wildcard, &query.policy, "device_groups", "group", "member", &[]).await.unwrap();
        acp.delete_relationship(&owner, acp::Subject::entity(follower), &query.policy, "device_groups", "group", "member", &[]).await.unwrap();
        assert!(!vera.verify_transport_sentinel(&query).await.unwrap());
        node.shutdown().await.unwrap();
    }).unwrap();
}

/// What one transaction of `count` creates cost at its commit.
struct Measured {
    commit: Duration,
    blocks: u64,
}

/// Create `count` documents in one transaction, commit it, and check that
/// every document is registered to its creator once the commit returns.
async fn creates_in_one_transaction(count: usize) -> Measured {
    let directory = tempfile::tempdir().unwrap();
    let node = node(directory.path()).await;
    let vera = node.vera_acp().expect("vera.rs document ACP").clone();
    let owner = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
    let stranger = identity::Did::new(STRANGER).unwrap();
    let policy = vera
        .add_policy(owner.as_str(), POLICY, None)
        .await
        .expect("policy");
    node.add_collection(&defra_core::RequestContext::ambient(), &users(&policy))
        .await
        .expect("collection");

    let transaction = node.begin_transaction_guard(false).await.unwrap();
    let mut docs = Vec::with_capacity(count);
    for index in 0..count {
        docs.push(created(
            transaction
                .execute_request(create_user(&format!("doc {index}"), &owner))
                .await,
        ));
    }
    let height = || vera.acp_light_client_status().expect("status").height;
    let before = height();
    let started = Instant::now();
    transaction.commit().await.expect("commit");
    let commit = started.elapsed();
    let blocks = height() - before;

    for doc in &docs {
        assert!(reads_doc(&node, &owner, &policy, doc).await, "{doc}");
        assert!(!reads_doc(&node, &stranger, &policy, doc).await, "{doc}");
    }
    assert!(node
        .owed_acp_as(
            &defra_core::RequestContext::default().with_identity(Some(owner.to_string())),
            owner.as_str()
        )
        .await
        .expect("what is owed")
        .is_empty());
    eprintln!("{count} creates in one transaction: commit {commit:?}, {blocks} block(s)");
    drop(vera);
    node.shutdown().await.expect("shutdown");
    Measured { commit, blocks }
}

/// The blocks `count` registrations take as lists, and one the heartbeat may
/// add meanwhile.
fn at_most(count: usize) -> u64 {
    count.div_ceil(vera::MAX_REGISTRATIONS_PER_REQUEST) as u64 + 1
}

#[test]
fn ten_creates_in_one_transaction_are_registered_in_one_block() {
    workers::block_on(async {
        let measured = creates_in_one_transaction(10).await;
        assert!(measured.blocks <= at_most(10), "{} blocks", measured.blocks);
    })
    .unwrap();
}

#[test]
fn thirty_creates_in_one_transaction_are_registered_in_one_block() {
    workers::block_on(async {
        let measured = creates_in_one_transaction(30).await;
        assert!(measured.blocks <= at_most(30), "{} blocks", measured.blocks);
        assert!(
            measured.commit < Duration::from_secs(3),
            "the commit took {:?}",
            measured.commit
        );
    })
    .unwrap();
}

/// More than a list holds goes in several, each within what vera.rs applies
/// in one block.
#[test]
fn more_creates_than_one_list_holds_are_registered_in_several() {
    workers::block_on(async {
        let count = vera::MAX_REGISTRATIONS_PER_REQUEST + 6;
        let measured = creates_in_one_transaction(count).await;
        assert!(
            measured.blocks <= at_most(count),
            "{} blocks",
            measured.blocks
        );
    })
    .unwrap();
}

#[test]
fn a_created_document_registers_with_the_relationship_its_bound_field_carries() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().unwrap();
        let owner = identity::Did::new(vera.self_did().unwrap()).unwrap();
        let child_resource = r#"
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
        let definition = format!("{POLICY}{child_resource}");
        let policy = vera.add_policy(owner.as_str(), &definition, None).await.unwrap();
        let children = format!(r#"type Job @governed(root: "jobs") @policy(id: "{policy}", resource: "jobs") {{ parent: Users @immutable @policy(relation: "parent") }}"#);
        node.add_collection(&defra_core::RequestContext::ambient(), &format!("{}\n{children}", users(&policy))).await.unwrap();
        let parent = created(node.execute_request(create_user("parent", &owner)).await);
        let mutation = format!(r#"mutation {{ add_Job(input: {{parent: "{parent}"}}) {{ _docID }} }}"#);
        let request = defra_node::QueryRequest::new(mutation).with_identity(Some(owner.clone()));
        let response = node.execute_request(request).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let data = response.data.unwrap();
        let child = data["add_Job"][0]["_docID"].as_str().unwrap();
        assert_eq!(node.document_acp().get_doc_owner(&policy, "jobs", child).await.unwrap(), Some(owner.clone()));
        // The relationship was stored with the registration: a reader of the
        // parent reads the child through `parent->read`.
        let reader = identity::Did::new(STRANGER).unwrap();
        assert!(!reads_resource(&node, &reader, &policy, "jobs", child).await);
        node.document_acp()
            .add_actor_relationship(&owner, &reader, &policy, "users", &parent, "reader", &[])
            .await
            .unwrap();
        assert!(reads_resource(&node, &reader, &policy, "jobs", child).await);
        node.shutdown().await.unwrap();
    }).unwrap();
}

async fn reads_resource(
    node: &defra_node::Node,
    actor: &identity::Did,
    policy: &str,
    resource: &str,
    doc: &str,
) -> bool {
    node.document_acp()
        .check_doc_access(
            &acp::Identity::Authenticated(actor.clone()),
            acp::DocumentPermission::Read,
            policy,
            resource,
            doc,
        )
        .await
        .unwrap()
}

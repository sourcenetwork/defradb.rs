//! The documents one transaction creates are registered on the Vera node
//! together: one block for up to a full list of them, where each used to take
//! a block of its own.

use std::time::{Duration, Instant};

use super::{create_user, created, node, reads_doc, users, POLICY, STRANGER};

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
    node.add_collection(&users(&policy))
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
fn a_created_document_registers_under_its_selected_parent() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().unwrap();
        let owner = identity::Did::new(vera.self_did().unwrap()).unwrap();
        let child_resource = r#"
  - name: jobs
    permissions: [{name: read}, {name: update}, {name: delete}]
    relations: [{name: parent, types: [users]}]
    create: {parent: parent, requires: owner}
"#;
        let definition = format!("{POLICY}{child_resource}");
        let policy = vera.add_policy(owner.as_str(), &definition, None).await.unwrap();
        let children = format!(r#"type Job @policy(id: "{policy}", resource: "jobs", parentField: "parentId") {{ parentId: String! }}"#);
        node.add_collection(&format!("{}\n{children}", users(&policy))).await.unwrap();
        let parent = created(node.execute_request(create_user("parent", &owner)).await);
        let mutation = format!(r#"mutation {{ add_Job(input: {{parentId: "{parent}"}}) {{ _docID }} }}"#);
        let request = defra_node::QueryRequest::new(mutation).with_identity(Some(owner.clone()));
        let response = node.execute_request(request).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let data = response.data.unwrap();
        let child = data["add_Job"][0]["_docID"].as_str().unwrap();
        assert_eq!(node.document_acp().get_doc_owner(&policy, "jobs", child).await.unwrap(), Some(owner));
        node.shutdown().await.unwrap();
    }).unwrap();
}

//! Owner lookups inside a transaction on a remote document ACP provider: an
//! owed registration names its creator from the transaction's own view; any
//! other owner is the provider's, confirmed again when the transaction
//! commits. Modeled by proofs/tla/RemoteOwnerObservation.tla.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use identity::Did;

use super::staged_relationship_tests::{
    actor, node_identity, provision, start, Remote, OWNER, READER,
};
use super::Node;

/// A node on `remote` with a policy-bound collection.
async fn node(remote: &Arc<Remote>) -> Node {
    let node = start(&node_identity(), Some(remote), None).await;
    provision(&node).await;
    node
}

fn create(name: &str) -> query::QueryRequest {
    query::QueryRequest::new(format!(
        "mutation {{ add_Record(input: {{name: \"{name}\"}}) {{ _docID }} }}"
    ))
    .with_identity(Some(Did::new(OWNER).unwrap()))
}

fn created(response: query::QueryResponse) -> String {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()["add_Record"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The owner `OWNER` is told of `doc` inside `txn`.
async fn owner_in(node: &Node, txn: &query::TransactionHandle, doc: &str) -> Option<String> {
    let rows = node
        .observe_document_owners_in_txn(
            txn,
            "Record",
            &[doc.to_owned()],
            Some(Did::new(OWNER).unwrap()),
        )
        .await
        .unwrap();
    assert!(rows[0].visible, "the owner reads its document");
    rows[0].owner.as_ref().map(|owner| owner.to_string())
}

/// How many records `OWNER` reads.
async fn records(node: &Node) -> usize {
    let response = node
        .execute_request(
            query::QueryRequest::new("{ Record { name } }")
                .with_identity(Some(Did::new(OWNER).unwrap())),
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()["Record"].as_array().unwrap().len()
}

#[test]
fn an_owner_lookup_in_a_transaction_answers_registered_created_and_owed_documents() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        let registered = created(node.execute_request(create("registered")).await);
        assert_eq!(remote.owners.get(&registered).as_deref(), Some(OWNER));

        remote.down.store(true, Ordering::SeqCst);
        let earlier = node.begin_transaction(false).await.unwrap();
        let owed = created(node.execute_request_in_txn(create("owed"), &earlier).await);
        node.commit_transaction(&earlier).await.unwrap();
        assert!(
            remote.owners.get(&owed).is_none(),
            "the registration is owed"
        );
        remote.down.store(false, Ordering::SeqCst);

        let txn = node.begin_transaction(false).await.unwrap();
        let fresh = created(node.execute_request_in_txn(create("fresh"), &txn).await);
        assert_eq!(
            owner_in(&node, &txn, &registered).await.as_deref(),
            Some(OWNER)
        );
        assert_eq!(owner_in(&node, &txn, &fresh).await.as_deref(), Some(OWNER));
        assert_eq!(owner_in(&node, &txn, &owed).await.as_deref(), Some(OWNER));
        node.commit_transaction(&txn).await.unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn an_owner_that_changed_before_the_commit_conflicts_and_commits_nothing() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        let doc = created(node.execute_request(create("moved")).await);
        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(owner_in(&node, &txn, &doc).await.as_deref(), Some(OWNER));
        created(node.execute_request_in_txn(create("decided"), &txn).await);
        // A transfer finalized on the provider, by somebody else.
        remote.owners.insert(doc.clone(), READER.to_owned());
        let error = node.commit_transaction(&txn).await.unwrap_err();
        assert!(
            matches!(error, query::TransactionError::Conflict(ref message) if message.contains("owner changed")),
            "{error:?}"
        );
        assert!(error.is_retryable());
        // The moved document is the other owner's now, and the one the
        // transaction wrote did not land.
        assert_eq!(records(&node).await, 0);
        assert!(node.commit_transaction(&txn).await.is_err(), "the transaction is gone");
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn an_unreachable_provider_refuses_the_commit_and_keeps_the_transaction() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        let doc = created(node.execute_request(create("kept")).await);
        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(owner_in(&node, &txn, &doc).await.as_deref(), Some(OWNER));
        created(node.execute_request_in_txn(create("later"), &txn).await);
        remote.down.store(true, Ordering::SeqCst);
        let error = node.commit_transaction(&txn).await.unwrap_err();
        assert!(error.is_retryable(), "{error:?}");
        assert!(
            error.to_string().contains("nothing was committed"),
            "{error}"
        );
        remote.down.store(false, Ordering::SeqCst);
        assert_eq!(records(&node).await, 1);
        node.commit_transaction(&txn).await.unwrap();
        assert_eq!(records(&node).await, 2);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_settle_of_the_owed_registration_conflicts_a_decision_made_on_its_creator() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        remote.down.store(true, Ordering::SeqCst);
        let earlier = node.begin_transaction(false).await.unwrap();
        let owed = created(node.execute_request_in_txn(create("owed"), &earlier).await);
        node.commit_transaction(&earlier).await.unwrap();

        let txn = node.begin_transaction(false).await.unwrap();
        assert_eq!(owner_in(&node, &txn, &owed).await.as_deref(), Some(OWNER));
        created(node.execute_request_in_txn(create("decided"), &txn).await);
        remote.down.store(false, Ordering::SeqCst);
        let settled = node
            .await_owed_acp_as(&actor(OWNER), &earlier, Duration::from_secs(20))
            .await
            .unwrap();
        assert!(matches!(settled, acp::OwedWait::Settled), "{settled:?}");
        assert_eq!(remote.owners.get(&owed).as_deref(), Some(OWNER));
        let error = node.commit_transaction(&txn).await.unwrap_err();
        assert!(
            matches!(error, query::TransactionError::Conflict(_)),
            "{error:?}"
        );
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_transaction_that_asked_nobody_commits_without_a_round() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        let txn = node.begin_transaction(false).await.unwrap();
        created(node.execute_request_in_txn(create("plain"), &txn).await);
        remote.down.store(true, Ordering::SeqCst);
        node.commit_transaction(&txn).await.unwrap();
        assert_eq!(records(&node).await, 1);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

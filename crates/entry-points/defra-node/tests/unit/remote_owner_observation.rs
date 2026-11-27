//! Owner lookups inside a transaction on a remote document ACP provider: an
//! owed registration names its creator from the transaction's own view; any
//! other owner is the provider's, confirmed again when the transaction
//! commits. Modeled by proofs/tla/RemoteOwnerObservation.tla.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use identity::Did;

use super::staged_relationship_tests::{node_identity, provision, start, Remote, OWNER, READER};
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
    owner_as(node, txn, OWNER, doc).await
}

/// The owner `asker` is told of `doc` inside `txn`.
async fn owner_as(
    node: &Node,
    txn: &query::TransactionHandle,
    asker: &str,
    doc: &str,
) -> Option<String> {
    let rows = node
        .with_transaction(txn.parse::<defra_core::TxnId>().unwrap())
        .observe_document_owners("Record", &[doc.to_owned()], Some(Did::new(asker).unwrap()))
        .await
        .unwrap();
    assert!(rows[0].visible, "the asker reads its document");
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

        // Created by somebody the node holds no key or delegation for: its
        // registration waits for authority, owed, and no drainer settles it.
        let owed = created(
            node.execute_request(
                query::QueryRequest::new(
                    "mutation { add_Record(input: {name: \"owed\"}) { _docID } }",
                )
                .with_identity(Some(Did::new(READER).unwrap())),
            )
            .await,
        );
        assert!(
            remote.owners.get(&owed).is_none(),
            "the registration is owed"
        );

        let txn = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
        let fresh = created(
            node.with_transaction(txn)
                .execute_request(create("fresh"))
                .await,
        );
        assert_eq!(
            owner_in(
                &node,
                &query::TransactionHandle::new(txn.to_string()),
                &registered
            )
            .await
            .as_deref(),
            Some(OWNER)
        );
        assert_eq!(
            owner_in(
                &node,
                &query::TransactionHandle::new(txn.to_string()),
                &fresh
            )
            .await
            .as_deref(),
            Some(OWNER)
        );
        assert_eq!(
            owner_as(
                &node,
                &query::TransactionHandle::new(txn.to_string()),
                READER,
                &owed
            )
            .await
            .as_deref(),
            Some(READER)
        );
        node.with_transaction(txn).commit().await.unwrap();
        assert!(
            remote.owners.get(&owed).is_none(),
            "still owed at the commit"
        );
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
        let txn = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
        assert_eq!(owner_in(&node, &query::TransactionHandle::new(txn.to_string()), &doc).await.as_deref(), Some(OWNER));
        created(node.with_transaction(txn).execute_request(create("decided")).await);
        // A transfer finalized on the provider, by somebody else.
        remote.owners.insert(doc.clone(), READER.to_owned());
        let error = node.with_transaction(txn).commit().await.unwrap_err();
        assert!(
            matches!(error, crate::Error::TransactionConflict(query::TransactionError::Conflict(ref message)) if message.contains("owner changed")),
            "{error:?}"
        );
        assert!(error.is_retryable());
        // The moved document is the other owner's now, and the one the
        // transaction wrote did not land.
        assert_eq!(records(&node).await, 0);
        assert!(node.with_transaction(txn).commit().await.is_err(), "the transaction is gone");
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
        let txn = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
        assert_eq!(
            owner_in(&node, &query::TransactionHandle::new(txn.to_string()), &doc)
                .await
                .as_deref(),
            Some(OWNER)
        );
        created(
            node.with_transaction(txn)
                .execute_request(create("later"))
                .await,
        );
        remote.down.store(true, Ordering::SeqCst);
        let error = node.with_transaction(txn).commit().await.unwrap_err();
        assert!(error.is_retryable(), "{error:?}");
        assert!(
            error.to_string().contains("nothing was committed"),
            "{error}"
        );
        remote.down.store(false, Ordering::SeqCst);
        assert_eq!(records(&node).await, 1);
        node.with_transaction(txn).commit().await.unwrap();
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
        let earlier = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
        let owed = created(
            node.with_transaction(earlier)
                .execute_request(create("owed"))
                .await,
        );
        node.with_transaction(earlier).commit().await.unwrap();

        let txn = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
        assert_eq!(
            owner_in(
                &node,
                &query::TransactionHandle::new(txn.to_string()),
                &owed
            )
            .await
            .as_deref(),
            Some(OWNER)
        );
        created(
            node.with_transaction(txn)
                .execute_request(create("decided"))
                .await,
        );
        remote.down.store(false, Ordering::SeqCst);
        let settled = node
            .with_identity(identity::Did::new(OWNER).unwrap())
            .await_owed_acp(earlier, Duration::from_secs(20))
            .await
            .unwrap();
        assert!(matches!(settled, acp::OwedWait::Settled), "{settled:?}");
        assert_eq!(remote.owners.get(&owed).as_deref(), Some(OWNER));
        let error = node.with_transaction(txn).commit().await.unwrap_err();
        assert!(
            matches!(error, crate::Error::TransactionConflict(_)),
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
        let txn = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
        created(
            node.with_transaction(txn)
                .execute_request(create("plain"))
                .await,
        );
        remote.down.store(true, Ordering::SeqCst);
        node.with_transaction(txn).commit().await.unwrap();
        assert_eq!(records(&node).await, 1);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// Holds the next provider owner read and drives `commit` until that read
/// waits: the commit, paused in its confirmation round.
async fn hold_the_round<'a, F, E>(remote: &Arc<Remote>, commit: F) -> std::pin::Pin<Box<F>>
where
    F: std::future::Future<Output = Result<(), E>> + 'a,
    E: std::fmt::Debug,
{
    remote.hold_owner_read.store(true, Ordering::SeqCst);
    let mut commit = Box::pin(commit);
    let held = futures::future::select(commit.as_mut(), Box::pin(remote.owner_read_waiting.wait()));
    match held.await {
        futures::future::Either::Left((result, _)) => panic!("committed unheld: {result:?}"),
        futures::future::Either::Right(_) => commit,
    }
}

#[test]
fn a_request_made_during_the_confirmation_round_finds_no_transaction() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        let doc = created(node.execute_request(create("kept")).await);
        let txn = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
        assert_eq!(
            owner_in(&node, &query::TransactionHandle::new(txn.to_string()), &doc)
                .await
                .as_deref(),
            Some(OWNER)
        );
        let commit = hold_the_round(&remote, node.with_transaction(txn).commit()).await;
        let late = node
            .with_transaction(txn)
            .execute_request(create("late"))
            .await;
        assert!(
            late.has_errors(),
            "a write after the commit began joined it"
        );
        assert!(
            node.with_transaction(txn)
                .observe_document_owners(
                    "Record",
                    std::slice::from_ref(&doc),
                    Some(Did::new(OWNER).unwrap())
                )
                .await
                .is_err(),
            "a lookup after the commit began joined it"
        );
        remote.owner_reads_released.set();
        commit.await.unwrap();
        assert_eq!(records(&node).await, 1, "the late write did not land");
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A commit the caller stops waiting for while its round waits on the
/// provider (the executor's own commit: the Node API runs a commit to its
/// end), then the same transaction committed again or discarded.
fn drop_the_round_then(commit_again: bool) {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        let doc = created(node.execute_request(create("kept")).await);
        let txn = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
        assert_eq!(
            owner_in(&node, &query::TransactionHandle::new(txn.to_string()), &doc)
                .await
                .as_deref(),
            Some(OWNER)
        );
        created(
            node.with_transaction(txn)
                .execute_request(create("pending"))
                .await,
        );
        let context = node.node_context();
        drop(
            hold_the_round(
                &remote,
                node.runner()
                    .commit_txn_as(&context, &query::TransactionHandle::new(txn.to_string())),
            )
            .await,
        );
        remote.owner_reads_released.set();
        if commit_again {
            node.with_transaction(txn).commit().await.unwrap();
            assert_eq!(records(&node).await, 2);
        } else {
            node.with_transaction(txn).discard().await.unwrap();
            assert_eq!(records(&node).await, 1);
        }
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

#[test]
fn a_commit_dropped_during_the_round_leaves_the_transaction_to_commit_again() {
    drop_the_round_then(true);
}

#[test]
fn a_commit_dropped_during_the_round_leaves_the_transaction_to_discard() {
    drop_the_round_then(false);
}

#[test]
fn a_guard_dropped_during_the_round_discards_its_transaction() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node(&remote).await;
        let doc = created(node.execute_request(create("kept")).await);
        let handle = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
        let guard = query::TransactionGuard::adopt(
            node.runner().as_ref(),
            node.node_context(),
            query::TransactionHandle::new(handle.clone().to_string()),
        );
        assert_eq!(
            owner_in(
                &node,
                &query::TransactionHandle::new(handle.to_string()),
                &doc
            )
            .await
            .as_deref(),
            Some(OWNER)
        );
        created(guard.execute(create("pending")).await);
        drop(hold_the_round(&remote, guard.commit()).await);
        remote.owner_reads_released.set();
        assert!(
            node.with_transaction(handle).commit().await.is_err(),
            "the guard let it go"
        );
        assert_eq!(records(&node).await, 1);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

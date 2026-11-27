//! Giving a document to another owner inside a node-owned transaction: on
//! local document ACP in the transaction, on a remote provider as an owed
//! change sent after the commit, behind the document's registration and the
//! transaction's earlier changes on it (`proofs/tla/OwnerTransfer.tla`).

use std::sync::atomic::Ordering;
use std::time::Duration;

use acp::{OwedOp, OwedState, OwedWait, OwedWhat};

use super::staged_relationship_tests::{
    create, created, eventually, node_identity, node_on, provision, reads, start, Remote, OWNER,
    READER,
};
use super::{Node, RelationshipChange};

const NEW_OWNER: &str = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";
const LATER: &str = "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";

fn did(value: &str) -> identity::Did {
    identity::Did::new(value).unwrap()
}

async fn transfer(
    node: &Node,
    handle: &defra_core::TxnId,
    from: &str,
    doc: &str,
    to: &str,
) -> crate::Result<()> {
    node.with_transaction(*handle)
        .acting_for(did(from))
        .transfer_dac_object_owner("Record", doc, to)
        .await
}

async fn grant(
    node: &Node,
    handle: &defra_core::TxnId,
    by: &str,
    doc: &str,
    to: &str,
) -> crate::Result<RelationshipChange> {
    node.with_transaction(*handle)
        .acting_for(did(by))
        .change_dac_actor_relationship("Record", doc, "reader", to, false)
        .await
}

async fn owed(node: &Node, actor: &str) -> Vec<acp::OwedEntry> {
    node.with_identity(did(actor))
        .owed_acp(actor)
        .await
        .unwrap()
}

fn not_owner<T>(answer: &crate::Result<T>) -> bool {
    matches!(answer, Err(crate::Error::Acp(acp::Error::NotOwner { .. })))
}

fn denied<T>(answer: &crate::Result<T>) -> bool {
    matches!(
        answer,
        Err(crate::Error::Acp(acp::Error::PermissionDenied(_)))
    )
}

/// How many commits of `doc` `did` reads in its commit history.
async fn commits(node: &Node, did: &str, doc: &str) -> usize {
    let response = node
        .execute_request(
            query::QueryRequest::new(format!(r#"{{ _commits(docID: "{doc}") {{ cid }} }}"#))
                .with_identity(Some(identity::Did::new(did).unwrap())),
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()["_commits"]
        .as_array()
        .map_or(0, Vec::len)
}

fn owner_of(remote: &Remote, doc: &str) -> Option<String> {
    remote.owners.get(doc)
}

/// The creator grants a reader and gives its new document away in the
/// create's transaction. The grant lands first, under the creator's
/// authority, and the transfer once, after it. The creator then reads and
/// manages nothing, and the new owner reads and grants.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_transfer_lands_once_after_the_creators_grant_and_hands_the_document_over() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        remote.keys.insert(NEW_OWNER.to_string(), ());
        let node = node_on(Some(&remote)).await;

        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        let doc = created(transaction.execute_request(create("given")).await);
        assert!(matches!(
            grant(&node, &handle, OWNER, &doc, READER).await.unwrap(),
            RelationshipChange::Staged {
                authority_checked: true,
                ..
            }
        ));
        transfer(&node, &handle, OWNER, &doc, NEW_OWNER)
            .await
            .unwrap();
        // The transfer is the creator's last change on the document there.
        assert!(denied(&grant(&node, &handle, OWNER, &doc, LATER).await));
        transaction.commit().await.unwrap();

        eventually("the transfer lands", || async {
            owner_of(&remote, &doc).as_deref() == Some(NEW_OWNER)
        })
        .await;
        assert_eq!(
            remote.take_writes(),
            [
                (true, doc.clone(), READER.to_string()),
                (true, doc.clone(), format!("owner {NEW_OWNER}")),
            ]
        );
        assert_eq!(remote.transfers.load(Ordering::SeqCst), 1);
        eventually("the transfer settles", || async {
            owed(&node, OWNER).await.is_empty()
        })
        .await;
        assert!(reads(&node, OWNER).await.is_empty());
        assert_eq!(reads(&node, NEW_OWNER).await, ["given"]);
        assert_eq!(reads(&node, READER).await, ["given"]);
        assert!(commits(&node, NEW_OWNER, &doc).await > 0);
        assert_eq!(commits(&node, OWNER, &doc).await, 0);

        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        assert!(not_owner(&grant(&node, &handle, OWNER, &doc, LATER).await));
        assert!(matches!(
            grant(&node, &handle, NEW_OWNER, &doc, LATER).await.unwrap(),
            RelationshipChange::Staged {
                authority_checked: true,
                ..
            }
        ));
        transaction.commit().await.unwrap();
        eventually("the new owner's grant lands", || async {
            remote.holds(&doc, LATER)
        })
        .await;
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// Only the owner may give a document away: a reader's transfer and the
/// intended new owner's own are refused before the commit, and the document
/// stays its creator's.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_transfer_by_anyone_but_the_owner_is_refused_before_the_commit() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        let node = node_on(Some(&remote)).await;
        let doc = created(node.execute_request(create("kept")).await);

        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        for stranger in [READER, NEW_OWNER] {
            let refused = transfer(&node, &handle, stranger, &doc, LATER).await;
            assert!(not_owner(&refused), "{stranger}: {refused:?}");
        }
        for malformed in ["*", "not a DID"] {
            let refused = transfer(&node, &handle, OWNER, &doc, malformed).await;
            assert!(
                matches!(refused, Err(crate::Error::InvalidNewOwner(_))),
                "{malformed}: {refused:?}"
            );
        }
        transaction.commit().await.unwrap();
        assert_eq!(owner_of(&remote, &doc).as_deref(), Some(OWNER));
        assert_eq!(reads(&node, OWNER).await, ["kept"]);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// The provider cannot be reached at the commit: the document's
/// registration and its transfer stay owed, the creator is denied and the
/// new owner is not let in. After a restart the registration lands first,
/// then the transfer, once.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn an_owed_registration_holds_the_transfer_which_lands_once_after_a_restart() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let directory = tempfile::tempdir().unwrap();
        let remote = Remote::new();
        let node_did = node_identity();
        let first = start(&node_did, Some(&remote), Some(directory.path())).await;
        provision(&first).await;

        remote.down.store(true, Ordering::SeqCst);
        let transaction = first.new_txn(false).await.unwrap();
        let handle = transaction.id();
        let doc = created(transaction.execute_request(create("held")).await);
        transfer(&first, &handle, OWNER, &doc, NEW_OWNER)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(owner_of(&remote, &doc), None);
        assert!(reads(&first, OWNER).await.is_empty());
        assert!(reads(&first, NEW_OWNER).await.is_empty());
        // The commit history follows the owed transfer as the document does.
        assert_eq!(commits(&first, OWNER, &doc).await, 0);
        assert_eq!(commits(&first, NEW_OWNER, &doc).await, 0);
        first.shutdown().await.unwrap();
        drop(first);

        remote.down.store(false, Ordering::SeqCst);
        let second = start(&node_did, Some(&remote), Some(directory.path())).await;
        eventually("the transfer lands after the registration", || async {
            owner_of(&remote, &doc).as_deref() == Some(NEW_OWNER)
        })
        .await;
        eventually("the transfer settles", || async {
            owed(&second, OWNER).await.is_empty()
        })
        .await;
        assert_eq!(remote.transfers.load(Ordering::SeqCst), 1);
        assert_eq!(
            remote.take_writes(),
            [(true, doc.clone(), format!("owner {NEW_OWNER}"))]
        );
        assert_eq!(reads(&second, NEW_OWNER).await, ["held"]);
        assert!(reads(&second, OWNER).await.is_empty());
        assert!(commits(&second, NEW_OWNER, &doc).await > 0);
        assert_eq!(commits(&second, OWNER, &doc).await, 0);
        second.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A transfer the provider refuses leaves the committed document its
/// creator's on the provider. The refusal is kept and reported, the creator
/// stays denied while it is kept, and reads again once it abandons it.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_refused_transfer_is_reported_and_leaves_the_document_its_creators() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        remote.refuse_transfers.store(true, Ordering::SeqCst);
        let node = node_on(Some(&remote)).await;
        let doc = created(node.execute_request(create("mine")).await);

        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        transfer(&node, &handle, OWNER, &doc, NEW_OWNER)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        let wait = node
            .with_identity(did(OWNER))
            .await_owed_acp(handle, Duration::from_secs(20))
            .await
            .unwrap();
        assert!(matches!(wait, OwedWait::Parked { .. }), "{wait:?}");
        let report = owed(&node, OWNER).await;
        let [entry] = &report[..] else {
            panic!("one entry: {report:?}");
        };
        let OwedWhat::Change { seq, op, .. } = &entry.what else {
            panic!("a change: {entry:?}");
        };
        assert_eq!(*op, OwedOp::Transfer);
        assert!(
            matches!(entry.state, OwedState::Refused { .. }),
            "{entry:?}"
        );
        assert_eq!(owner_of(&remote, &doc).as_deref(), Some(OWNER));
        assert_eq!(remote.transfers.load(Ordering::SeqCst), 0);
        assert!(reads(&node, OWNER).await.is_empty());

        node.with_identity(did(OWNER))
            .abandon_owed_acp_change(*seq)
            .await
            .unwrap();
        assert_eq!(reads(&node, OWNER).await, ["mine"]);
        assert!(reads(&node, NEW_OWNER).await.is_empty());
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// A transfer that took effect while its answer was lost is sent again and
/// refused, its creator owning nothing any more; the node finds the new
/// owner named and settles it. It took effect once and is never reported
/// refused.
#[test]
#[mantra_macros::req_test("defra.acp.vera_owner_transfer")]
fn a_transfer_whose_answer_was_lost_settles_once_and_is_not_reported_refused() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let remote = Remote::new();
        remote.lose_transfer_answer.store(true, Ordering::SeqCst);
        let node = node_on(Some(&remote)).await;
        let doc = created(node.execute_request(create("moved")).await);

        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        transfer(&node, &handle, OWNER, &doc, NEW_OWNER)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        eventually("the transfer settles", || async {
            owed(&node, OWNER).await.is_empty()
        })
        .await;
        assert_eq!(owner_of(&remote, &doc).as_deref(), Some(NEW_OWNER));
        assert_eq!(remote.transfers.load(Ordering::SeqCst), 1);
        assert_eq!(reads(&node, NEW_OWNER).await, ["moved"]);
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

/// On the node's own document ACP the owner tuple is replaced in the
/// transaction: the new owner reads and grants once it commits, the old
/// owner and a reader may do neither, and a rolled-back transfer leaves the
/// owner as it was.
#[test]
#[mantra_macros::req_test("defra.acp.local_owner_transfer")]
fn a_local_transfer_replaces_the_owner_in_the_transaction() {
    workers::block_on(async {
        let _serial = super::tests::SIGNING_STORE_GUARD.lock().await;
        let node = node_on(None).await;
        let doc = created(node.execute_request(create("local")).await);

        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        assert!(not_owner(
            &transfer(&node, &handle, READER, &doc, READER).await
        ));
        transfer(&node, &handle, OWNER, &doc, NEW_OWNER)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        assert!(reads(&node, OWNER).await.is_empty());
        assert_eq!(reads(&node, NEW_OWNER).await, ["local"]);

        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        assert!(not_owner(&grant(&node, &handle, OWNER, &doc, READER).await));
        assert!(matches!(
            grant(&node, &handle, NEW_OWNER, &doc, READER)
                .await
                .unwrap(),
            RelationshipChange::Applied { existed: false }
        ));
        assert!(not_owner(
            &transfer(&node, &handle, OWNER, &doc, OWNER).await
        ));
        transaction.commit().await.unwrap();
        assert_eq!(reads(&node, READER).await, ["local"]);

        let transaction = node.new_txn(false).await.unwrap();
        let handle = transaction.id();
        transfer(&node, &handle, NEW_OWNER, &doc, OWNER)
            .await
            .unwrap();
        transaction.discard().await.unwrap();
        assert_eq!(reads(&node, NEW_OWNER).await, ["local"]);
        assert!(reads(&node, OWNER).await.is_empty());
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

//! A deleted document's archive, while it is owed
//! (`proofs/tla/ArchiveOnDelete.tla`): it denies every read of the document
//! but one, its actor's read of the commit history, and a delete in a
//! transaction records it under that transaction.

use std::sync::atomic::Ordering;

use acp::{
    DocumentACP as _, DocumentPermission, Identity, OwedDenial, OwedDenials, OwedFilter, OwedOp,
    OwedTuple, OwedWhat,
};
use query::prepared::PreparedInputMemo;
use query::txn::AcpRegistrationProjection;
use query::QueryExecutor as _;

use super::external_outbox::{actor, owed, tuple, Outbox};
use super::external_registration::{alice, bob, request, RemoteAcp, POLICY, RESOURCE};
use super::external_staging::{Change, Staging};

const DOC: &str = "doc-1";
const NOT_OWNER: &str = "permission denied: UNAUTHORIZED: not document owner, cannot archive the deleted document's object";

/// Owe, as alice, the archive of [`DOC`].
async fn archive(outbox: &Outbox) {
    let (txn, changes) = outbox.begin().await;
    let tuple = OwedTuple::archive(POLICY, RESOURCE, DOC);
    changes
        .stage_restored(owed(&tuple, OwedOp::Archive))
        .await
        .unwrap();
    txn.commit().await.unwrap();
}

/// Whether `identity` reads [`DOC`], or its commit history, as a new
/// transaction decides it, with and without a request memo.
async fn reads(outbox: &Outbox, identity: &Identity, history: bool) -> bool {
    let (txn, _) = outbox.begin().await;
    let view = txn.acpstore();
    let projection = AcpRegistrationProjection::new(acp::PendingRegistrations::new(&view))
        .with_owed_denials(OwedDenials::new(&view));
    let live = query::txn::check_doc_access_with_projection(
        &*outbox.gate,
        Some(&projection),
        identity,
        DocumentPermission::Read,
        POLICY,
        RESOURCE,
        DOC,
        history,
    )
    .await
    .unwrap();
    let prepared = query::txn::check_doc_access_with_prepared_inputs(
        &*outbox.gate,
        Some(&projection),
        identity,
        DocumentPermission::Read,
        POLICY,
        RESOURCE,
        DOC,
        history,
        Some(&PreparedInputMemo::default()),
    )
    .await
    .unwrap();
    assert_eq!(live, prepared, "both paths decide alike");
    drop(view);
    txn.discard().unwrap();
    live
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_delete_archive")]
fn an_owed_archive_leaves_its_actor_the_commit_history_and_nothing_else() {
    workers::block_on(async {
        let outbox = Outbox::new();
        archive(&outbox).await;
        let (alice, bob) = (
            Identity::Authenticated(alice()),
            Identity::Authenticated(bob()),
        );

        let (txn, _) = outbox.begin().await;
        for identity in [&alice, &bob, &Identity::Anonymous] {
            assert_eq!(
                OwedDenials::new(&txn.acpstore())
                    .on(&*RemoteAcp::new(), identity, POLICY, RESOURCE, DOC)
                    .await
                    .unwrap(),
                OwedDenial::Archived
            );
        }
        txn.discard().unwrap();

        assert!(reads(&outbox, &alice, true).await, "its actor's history");
        assert!(!reads(&outbox, &alice, false).await, "the document itself");
        assert!(!reads(&outbox, &bob, true).await, "another actor's history");
        assert!(!reads(&outbox, &bob, false).await);
        assert!(!reads(&outbox, &Identity::Anonymous, true).await);
        assert!(!reads(&outbox, &Identity::Anonymous, false).await);
    })
    .unwrap()
}

#[test]
fn an_owed_delete_of_the_actors_owner_relation_takes_its_commit_history_too() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let owner = tuple(DOC, acp::OWNER_RELATION, actor(alice()));
        outbox.commit(&owner, OwedOp::Delete).await;
        archive(&outbox).await;
        let alice = Identity::Authenticated(alice());

        assert!(!reads(&outbox, &alice, true).await);
        assert!(!reads(&outbox, &alice, false).await);
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_delete_archive")]
fn a_delete_by_anyone_but_the_owner_is_refused_in_these_words() {
    workers::block_on(async {
        let staging = Staging::new().await;
        staging.remote.archives.store(true, Ordering::SeqCst);
        let doc = staging.create(alice(), "kept", None).await;
        staging
            .on_provider(alice(), Change::new(alice(), &doc, OwedOp::Add, bob()))
            .await;

        let refused = staging
            .node
            .runner
            .execute(request(
                &format!(r#"mutation {{ delete_Users(docID: "{doc}") {{ _docID }} }}"#),
                Some(bob()),
            ))
            .await;
        let messages: Vec<&str> = refused
            .errors
            .iter()
            .map(|error| error.message.as_str())
            .collect();
        assert_eq!(messages, [NOT_OWNER]);
        let owed = staging
            .gate
            .owed_entries(OwedFilter::Actor(&bob()))
            .await
            .unwrap();
        assert!(owed.is_empty(), "{owed:?}");
        assert_eq!(staging.remote.owner(&doc), Some(alice()));
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_delete_archive")]
fn a_delete_in_a_transaction_owes_its_archive_under_that_transaction() {
    workers::block_on(async {
        let staging = Staging::new().await;
        staging.remote.archives.store(true, Ordering::SeqCst);
        let doc = staging.create(alice(), "doomed", None).await;

        let handle = staging.begin().await;
        let deleted = staging
            .node
            .runner
            .execute_in_txn(
                request(
                    &format!(r#"mutation {{ delete_Users(docID: "{doc}") {{ _docID }} }}"#),
                    Some(alice()),
                ),
                &handle,
            )
            .await;
        assert!(!deleted.has_errors(), "{:?}", deleted.errors);
        staging.commit(&handle).await;

        let txn: u64 = handle.as_str().parse().unwrap();
        let owed = staging
            .gate
            .owed_entries(OwedFilter::Transaction(txn))
            .await
            .unwrap();
        assert_eq!(owed.len(), 1, "the wait for the transaction finds it");
        assert!(
            matches!(
                owed[0].what,
                OwedWhat::Change {
                    op: OwedOp::Archive,
                    ..
                }
            ),
            "{owed:?}"
        );
        assert_eq!((owed[0].object.as_str(), owed[0].txn), (&*doc, Some(txn)));
    })
    .unwrap()
}

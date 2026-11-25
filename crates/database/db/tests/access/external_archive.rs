//! A deleted document's archive, while it is owed
//! (`proofs/tla/ArchiveOnDelete.tla`): it is made as the document's owner,
//! whoever the policy let delete, it denies every read of the document but
//! one, a deleting owner's read of the commit history, and a delete in a
//! transaction records it under that transaction.

use std::sync::atomic::Ordering;

use acp::{
    DocumentACP as _, DocumentPermission, Identity, OwedDenial, OwedDenials, OwedFilter, OwedOp,
    OwedTuple, OwedWhat,
};
use datastore::BasicTxn;
use identity::Did;
use query::prepared::PreparedInputMemo;
use query::txn::AcpRegistrationProjection;
use query::{QueryExecutor as _, QueryRequest};

use super::external_outbox::{actor, owed, tuple, Outbox};
use super::external_registration::{alice, bob, request, RemoteAcp, POLICY, RESOURCE};
use super::external_staging::{carol, Change, Staging};

const DOC: &str = "doc-1";

/// Owe, as alice, the archive of [`DOC`], staged by her own delete.
async fn archive(outbox: &Outbox) {
    let (txn, changes) = outbox.begin().await;
    let tuple = OwedTuple::archive(POLICY, RESOURCE, DOC);
    changes
        .stage_archive(owed(&tuple, OwedOp::Archive), true)
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

/// Whether `identity` reads [`DOC`], or its commit history, through the
/// whole read rule (the registration check included), live or with
/// `inputs`. [`DOC`] is registered nowhere: its creator's registration is
/// still owed, or the provider no longer has it.
async fn rule(
    outbox: &Outbox,
    identity: &Identity,
    history: bool,
    inputs: Option<&PreparedInputMemo>,
) -> query::error::Result<bool> {
    let (txn, _) = outbox.begin().await;
    let view = txn.acpstore();
    let projection = AcpRegistrationProjection::new(acp::PendingRegistrations::new(&view))
        .with_owed_denials(OwedDenials::new(&view));
    let context = defra_core::RequestContext::default();
    let (projection_ref, gate) = (Some(&projection), &*outbox.gate);
    let allowed = if history {
        query::txn::check_commit_read_access_as(
            &context,
            gate,
            projection_ref,
            identity,
            POLICY,
            RESOURCE,
            "",
            false,
            DOC,
            inputs,
        )
        .await
    } else {
        query::txn::check_doc_read_access_as(
            &context,
            gate,
            projection_ref,
            identity,
            POLICY,
            RESOURCE,
            "",
            false,
            DOC,
            inputs,
        )
        .await
    };
    drop(view);
    txn.discard().unwrap();
    allowed
}

/// [`rule`] decided alike live and with an empty request memo: an owed
/// archive needs nothing from the provider.
async fn reads_through_the_rule(outbox: &Outbox, identity: &Identity, history: bool) -> bool {
    let live = rule(outbox, identity, history, None).await.unwrap();
    let memo = PreparedInputMemo::default();
    let prepared = rule(outbox, identity, history, Some(&memo)).await.unwrap();
    assert_eq!(live, prepared, "both paths decide alike");
    live
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_delete_archive")]
fn an_owed_archive_denies_before_an_owed_registration_or_an_unregistered_object_could_grant() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let (alice, bob) = (
            Identity::Authenticated(alice()),
            Identity::Authenticated(bob()),
        );
        assert!(
            rule(&outbox, &bob, false, None).await.unwrap(),
            "an object the provider does not have is public"
        );
        archive(&outbox).await;
        for identity in [&alice, &bob, &Identity::Anonymous] {
            assert!(!reads_through_the_rule(&outbox, identity, false).await);
        }
        assert!(reads_through_the_rule(&outbox, &alice, true).await);
        assert!(!reads_through_the_rule(&outbox, &bob, true).await);

        let (txn, _) = outbox.begin().await;
        acp::PendingRegistrations::new(&txn.acpstore())
            .record(&acp::PendingRegistration::new(
                alice.did().unwrap().clone(),
                POLICY,
                RESOURCE,
                DOC,
            ))
            .await
            .unwrap();
        txn.commit().await.unwrap();
        assert!(
            !reads_through_the_rule(&outbox, &alice, false).await,
            "the registration still owed to alice grants her nothing"
        );
        assert!(!reads(&outbox, &alice, false).await);
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_delete_archive")]
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

fn delete(doc: &str, actor: Did) -> QueryRequest {
    request(
        &format!(r#"mutation {{ delete_Users(docID: "{doc}") {{ _docID }} }}"#),
        Some(actor),
    )
}

/// The names `actor` reads in `Users`.
async fn names(staging: &Staging, actor: Option<Did>) -> Vec<String> {
    let read = staging
        .node
        .runner
        .execute(request("query { Users { name } }", actor))
        .await;
    assert!(!read.has_errors(), "{:?}", read.errors);
    read.data.unwrap()["Users"]
        .as_array()
        .unwrap()
        .iter()
        .map(|user| user["name"].as_str().unwrap().to_owned())
        .collect()
}

/// The archive alice owes on `doc`: its actor and state.
async fn owed_archive(staging: &Staging, doc: &str) -> Vec<(Did, acp::OwedState)> {
    staging
        .gate
        .owed_entries(OwedFilter::Actor(&alice()))
        .await
        .unwrap()
        .into_iter()
        .filter(|entry| {
            entry.object == doc
                && matches!(
                    entry.what,
                    OwedWhat::Change {
                        op: OwedOp::Archive,
                        ..
                    }
                )
        })
        .map(|entry| (entry.actor, entry.state))
        .collect()
}

/// Alice's document, which the policy lets bob delete too.
async fn shared_with_bob(staging: &Staging) -> String {
    staging.remote.archives.store(true, Ordering::SeqCst);
    let doc = staging.create(alice(), "doomed", None).await;
    staging
        .on_provider(alice(), Change::new(alice(), &doc, OwedOp::Add, bob()))
        .await;
    assert_eq!(names(staging, Some(bob())).await, ["doomed"]);
    doc
}

/// Bob deletes `doc`, which he does not own.
async fn delete_as_bob(staging: &Staging, doc: &str) {
    let deleted = staging.node.runner.execute(delete(doc, bob())).await;
    assert!(!deleted.has_errors(), "{:?}", deleted.errors);
    assert_eq!(
        deleted.data.unwrap()["delete_Users"][0]["_docID"],
        serde_json::json!(doc)
    );
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_delete_archive")]
fn a_permitted_non_owners_delete_commits_and_denies_the_document_to_everyone_at_once() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = shared_with_bob(&staging).await;
        delete_as_bob(&staging, &doc).await;
        assert_eq!(
            owed_archive(&staging, &doc).await,
            [(alice(), acp::OwedState::Staged)],
            "the archive is owed as the owner"
        );
        let bobs = staging
            .gate
            .owed_entries(OwedFilter::Actor(&bob()))
            .await
            .unwrap();
        assert!(bobs.is_empty(), "the deleter owes nothing: {bobs:?}");
        for actor in [Some(alice()), Some(bob()), Some(carol()), None] {
            assert!(names(&staging, actor).await.is_empty());
        }
        let txn = BasicTxn::new(staging.store.as_ref(), 0, true)
            .await
            .unwrap();
        assert_eq!(
            OwedDenials::new(&txn.acpstore())
                .archive_actor(POLICY, RESOURCE, &doc)
                .await
                .unwrap(),
            None,
            "an owner that did not delete keeps no commit history, nor does bob"
        );
        txn.discard().unwrap();

        staging
            .gate
            .settle_owed_changes_once(acp::OwedScope::All)
            .await
            .unwrap();
        let left = owed_archive(&staging, &doc).await;
        assert!(left.is_empty(), "{left:?}");
        assert_eq!(staging.remote.owner(&doc), None, "archived as alice");
        assert!(!staging.remote.holds(&doc, &bob()));
        for actor in [Some(alice()), Some(bob())] {
            assert!(names(&staging, actor).await.is_empty());
        }
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_delete_archive")]
fn a_non_owners_delete_parks_the_archive_until_the_owners_authority_appears() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = shared_with_bob(&staging).await;
        // From here the node signs only with the keys it holds: bob's.
        staging.remote.credentials.store(true, Ordering::SeqCst);
        staging.remote.hold_key(&bob());
        delete_as_bob(&staging, &doc).await;
        let parked = owed_archive(&staging, &doc).await;
        assert_eq!(parked.len(), 1, "{parked:?}");
        assert_eq!(parked[0].0, alice());
        assert_eq!(
            parked[0].1,
            acp::OwedState::AwaitingAuthority,
            "bob's key does not archive alice's object"
        );
        for actor in [Some(alice()), Some(bob()), None] {
            assert!(names(&staging, actor).await.is_empty());
        }
        staging
            .gate
            .settle_owed_changes_once(acp::OwedScope::All)
            .await
            .unwrap();
        assert_eq!(staging.remote.owner(&doc), Some(alice()), "nothing sent");

        let entry = staging
            .gate
            .owed_entries(OwedFilter::Actor(&alice()))
            .await
            .unwrap()
            .into_iter()
            .find(|entry| {
                matches!(
                    entry.what,
                    OwedWhat::Change {
                        op: OwedOp::Archive,
                        ..
                    }
                )
            })
            .expect("the archive is owed")
            .reference();
        assert!(
            staging.gate.reauthorize_owed(&bob(), &entry).await.is_err(),
            "bob owes nothing"
        );
        staging.remote.hold_key(&alice());
        staging
            .gate
            .reauthorize_owed(&alice(), &entry)
            .await
            .expect("the owner's authority is handed in");
        staging
            .gate
            .settle_owed_changes_once(acp::OwedScope::All)
            .await
            .unwrap();
        assert!(owed_archive(&staging, &doc).await.is_empty());
        assert_eq!(staging.remote.owner(&doc), None, "archived as alice");
        for actor in [Some(alice()), Some(bob())] {
            assert!(names(&staging, actor).await.is_empty());
        }
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_delete_archive")]
fn an_actor_without_the_delete_permission_is_refused_before_commit() {
    workers::block_on(async {
        let staging = Staging::new().await;
        staging.remote.archives.store(true, Ordering::SeqCst);
        let doc = staging.create(alice(), "kept", None).await;

        let refused = staging.node.runner.execute(delete(&doc, carol())).await;
        assert_eq!(
            refused.data.map(|data| data["delete_Users"].clone()),
            Some(serde_json::json!([])),
            "carol deletes nothing: {:?}",
            refused.errors
        );
        assert!(owed_archive(&staging, &doc).await.is_empty());
        assert_eq!(staging.remote.owner(&doc), Some(alice()));
        assert_eq!(names(&staging, Some(alice())).await, ["kept"]);
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

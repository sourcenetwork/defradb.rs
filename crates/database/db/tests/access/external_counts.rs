//! What a node owes is counted in a staged part and a settled part (#3070):
//! settling an owed change aborts no transaction it cannot make wrong, a
//! revocation committed after a decision still refuses it, the deny overlay
//! covers every kind of subject in two point reads, and the bounds hold.

use acp::outbox::VALIDATED_WITHIN_OF_NODE_LIMIT;
use acp::{Identity, NewOwedChange, OwedDenial, OwedDenials, OwedOp, OwedSubject};
use datastore::BasicTxn;
use identity::Did;

use super::external_outbox::{actor, is_conflict, owed, tuple, Outbox};
use super::external_registration::{alice, bob, mallory, RemoteAcp, POLICY, RESOURCE};
use super::external_staging::carol;

fn as_actor(did: Did) -> Identity {
    Identity::Authenticated(did)
}

/// The denial owed deletes put on `identity` on `object`, as `txn` reads it.
async fn denial(txn: &BasicTxn, identity: &Identity, object: &str) -> OwedDenial {
    OwedDenials::new(&txn.acpstore())
        .on(&*RemoteAcp::new(), identity, POLICY, RESOURCE, object)
        .await
        .unwrap()
}

/// What `outbox` denies `identity` on `object` as committed now.
async fn committed_denial(outbox: &Outbox, identity: &Identity, object: &str) -> OwedDenial {
    let (txn, _) = outbox.begin().await;
    let answer = denial(&txn, identity, object).await;
    txn.discard().unwrap();
    answer
}

/// Stage `change` in a transaction of its own and commit it.
async fn commit_new(outbox: &Outbox, change: NewOwedChange) -> u64 {
    let (txn, changes) = outbox.begin().await;
    let seq = changes.stage(change).await.unwrap().seq;
    txn.commit().await.unwrap();
    seq
}

/// Settle `first`'s change while a transaction that stages `second` is open,
/// then commit that transaction: whether it conflicts.
async fn stage_while_settling(
    outbox: &Outbox,
    first: NewOwedChange,
    second: NewOwedChange,
) -> bool {
    let settled = commit_new(outbox, first).await;
    let (txn, changes) = outbox.begin().await;
    changes.stage(second).await.unwrap();
    assert!(outbox.settle(settled).await);
    txn.commit().await.err().is_some_and(is_conflict)
}

#[test]
fn staging_another_tuple_of_a_document_while_one_settles_commits() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let first = owed(&tuple("doc-1", "reader", actor(bob())), OwedOp::Add);
        let second = owed(&tuple("doc-1", "reader", actor(mallory())), OwedOp::Delete);
        assert!(!stage_while_settling(&outbox, first, second).await);
    })
    .unwrap()
}

#[test]
fn staging_the_same_tuple_while_its_earlier_change_settles_commits() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));
        let conflicted = stage_while_settling(
            &outbox,
            owed(&reader, OwedOp::Add),
            owed(&reader, OwedOp::Delete),
        )
        .await;
        assert!(!conflicted);
        let (state, on_object, on_node) = outbox.committed(&reader).await;
        let state = state.expect("the delete is owed");
        assert_eq!((state.unsettled, state.deletes), (1, 1));
        assert_eq!((on_object, on_node), (1, 1));
    })
    .unwrap()
}

#[test]
fn staging_on_another_document_while_one_settles_commits() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let first = owed(&tuple("doc-1", "reader", actor(bob())), OwedOp::Delete);
        let second = owed(&tuple("doc-2", "reader", actor(bob())), OwedOp::Add);
        assert!(!stage_while_settling(&outbox, first, second).await);
    })
    .unwrap()
}

/// Two transactions staging one tuple are still ordered: the later commit
/// conflicts, and its re-run takes the later sequence.
#[test]
fn two_transactions_staging_one_tuple_still_conflict() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));
        let (first, first_changes) = outbox.begin().await;
        let (second, second_changes) = outbox.begin().await;
        first_changes
            .stage(owed(&reader, OwedOp::Add))
            .await
            .unwrap();
        second_changes
            .stage(owed(&reader, OwedOp::Delete))
            .await
            .unwrap();
        first.commit().await.unwrap();
        assert!(second.commit().await.err().is_some_and(is_conflict));
    })
    .unwrap()
}

/// A writing transaction that decided a denial commits past a settle on the
/// document: the settle can only lift a denial.
#[test]
fn a_decision_commits_past_a_settle_on_its_document() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let bob_reader = tuple("doc-1", "reader", actor(bob()));
        let carol_reader = tuple("doc-1", "reader", actor(carol()));
        let settled = outbox.commit(&carol_reader, OwedOp::Delete).await;
        outbox.commit(&bob_reader, OwedOp::Delete).await;

        let (txn, changes) = outbox.begin().await;
        assert_eq!(
            denial(&txn, &as_actor(bob()), "doc-1").await,
            OwedDenial::UnlessOwner
        );
        assert_eq!(
            denial(&txn, &as_actor(alice()), "doc-1").await,
            OwedDenial::None
        );
        changes
            .stage(owed(&tuple("doc-2", "reader", actor(bob())), OwedOp::Add))
            .await
            .unwrap();
        assert!(outbox.settle(settled).await);
        // The settle was of carol's delete: bob is still denied in the
        // transaction and after it.
        assert_eq!(
            denial(&txn, &as_actor(bob()), "doc-1").await,
            OwedDenial::UnlessOwner
        );
        txn.commit().await.unwrap();
        assert_eq!(
            committed_denial(&outbox, &as_actor(bob()), "doc-1").await,
            OwedDenial::UnlessOwner
        );
        assert_eq!(
            committed_denial(&outbox, &as_actor(carol()), "doc-1").await,
            OwedDenial::None
        );
    })
    .unwrap()
}

/// A revocation committed after a decision that let a reader in refuses that
/// decision at commit, whatever its subject: never a missed revoke.
#[test]
fn a_revocation_committed_after_a_decision_refuses_it() {
    workers::block_on(async {
        let revocations = [
            tuple("doc-1", "reader", actor(bob())),
            tuple("doc-1", acp::OWNER_RELATION, actor(bob())),
            tuple("doc-1", "reader", OwedSubject::Wildcard),
            tuple(
                "doc-1",
                "reader",
                OwedSubject::EntitySet {
                    resource: "groups".into(),
                    object: "team".into(),
                    relation: "member".into(),
                },
            ),
        ];
        for already_owed in [false, true] {
            for revocation in &revocations {
                let outbox = Outbox::new();
                if already_owed {
                    // Something is owed on the document, so its count exists.
                    outbox
                        .commit(&tuple("doc-1", "reader", actor(carol())), OwedOp::Add)
                        .await;
                }
                let (txn, changes) = outbox.begin().await;
                assert_eq!(
                    denial(&txn, &as_actor(bob()), "doc-1").await,
                    OwedDenial::None
                );
                changes
                    .stage(owed(&tuple("doc-2", "reader", actor(bob())), OwedOp::Add))
                    .await
                    .unwrap();
                outbox.commit(revocation, OwedOp::Delete).await;
                assert!(
                    txn.commit().await.err().is_some_and(is_conflict),
                    "a decision committed past {revocation:?}"
                );
                assert_ne!(
                    committed_denial(&outbox, &as_actor(bob()), "doc-1").await,
                    OwedDenial::None
                );
            }
        }
    })
    .unwrap()
}

/// A grant committed after a decision does not refuse it: it touches no
/// count of deletes.
#[test]
fn a_grant_committed_after_a_decision_does_not_refuse_it() {
    workers::block_on(async {
        let outbox = Outbox::new();
        outbox
            .commit(&tuple("doc-1", "reader", actor(carol())), OwedOp::Add)
            .await;
        let (txn, changes) = outbox.begin().await;
        assert_eq!(
            denial(&txn, &as_actor(bob()), "doc-1").await,
            OwedDenial::None
        );
        changes
            .stage(owed(&tuple("doc-2", "reader", actor(bob())), OwedOp::Add))
            .await
            .unwrap();
        outbox
            .commit(&tuple("doc-1", "reader", actor(bob())), OwedOp::Add)
            .await;
        txn.commit().await.unwrap();
    })
    .unwrap()
}

/// A delete's subject and relation, and what it denies three identities.
type Case = (OwedSubject, &'static str, [(Identity, OwedDenial); 3]);

/// Whom each kind of owed delete denies: the actor it names unless it owns
/// the document, the actor whose owner relation it takes always, and, for a
/// wildcard or a set, everyone but the owner, anonymous readers included.
#[test]
fn every_kind_of_subject_is_denied_as_its_delete_says() {
    workers::block_on(async {
        let cases: [Case; 4] = [
            (
                actor(bob()),
                "reader",
                [
                    (as_actor(bob()), OwedDenial::UnlessOwner),
                    (as_actor(carol()), OwedDenial::None),
                    (Identity::Anonymous, OwedDenial::None),
                ],
            ),
            (
                actor(bob()),
                acp::OWNER_RELATION,
                [
                    (as_actor(bob()), OwedDenial::Always),
                    (as_actor(carol()), OwedDenial::None),
                    (Identity::Anonymous, OwedDenial::None),
                ],
            ),
            (
                OwedSubject::Wildcard,
                "reader",
                [
                    (as_actor(bob()), OwedDenial::UnlessOwner),
                    (as_actor(carol()), OwedDenial::UnlessOwner),
                    (Identity::Anonymous, OwedDenial::UnlessOwner),
                ],
            ),
            (
                OwedSubject::EntitySet {
                    resource: "groups".into(),
                    object: "team".into(),
                    relation: "member".into(),
                },
                "reader",
                [
                    (as_actor(bob()), OwedDenial::UnlessOwner),
                    (as_actor(carol()), OwedDenial::UnlessOwner),
                    (Identity::Anonymous, OwedDenial::UnlessOwner),
                ],
            ),
        ];
        for (subject, relation, expected) in cases {
            let outbox = Outbox::new();
            let seq = outbox
                .commit(&tuple("doc-1", relation, subject.clone()), OwedOp::Delete)
                .await;
            for (identity, denied) in &expected {
                assert_eq!(
                    committed_denial(&outbox, identity, "doc-1").await,
                    *denied,
                    "{subject:?} {relation} {identity:?}"
                );
                assert_eq!(
                    committed_denial(&outbox, identity, "doc-2").await,
                    OwedDenial::None
                );
            }
            // An add staged after the delete does not lift the denial.
            outbox
                .commit(&tuple("doc-1", relation, subject.clone()), OwedOp::Add)
                .await;
            for (identity, denied) in &expected {
                assert_eq!(committed_denial(&outbox, identity, "doc-1").await, *denied);
            }
            // Settled, the delete denies no one.
            assert!(outbox.settle(seq).await);
            for (identity, _) in &expected {
                assert_eq!(
                    committed_denial(&outbox, identity, "doc-1").await,
                    OwedDenial::None
                );
            }
        }
    })
    .unwrap()
}

/// A fully settled tuple, object and node read as nothing owed, though their
/// counts stay as merged values; a rolled-back transaction leaves nothing.
#[test]
fn what_is_fully_settled_or_rolled_back_owes_nothing() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));
        let first = outbox.commit(&reader, OwedOp::Delete).await;
        let second = outbox.commit(&reader, OwedOp::Add).await;
        assert!(outbox.settle(first).await);
        assert!(outbox.settle(second).await);
        assert_eq!(outbox.committed(&reader).await, (None, 0, 0));
        assert_eq!(
            committed_denial(&outbox, &as_actor(bob()), "doc-1").await,
            OwedDenial::None
        );

        let (txn, changes) = outbox.begin().await;
        changes.stage(owed(&reader, OwedOp::Delete)).await.unwrap();
        txn.discard().unwrap();
        assert_eq!(outbox.committed(&reader).await, (None, 0, 0));
        assert_eq!(
            committed_denial(&outbox, &as_actor(bob()), "doc-1").await,
            OwedDenial::None
        );
        assert!(outbox.queue().await.is_empty());
    })
    .unwrap()
}

/// Near the node's bound two transactions staging under identities of their
/// own commit one after the other, and a settle aborts neither; far from it
/// they do not conflict.
#[test]
fn near_the_node_bound_stagers_are_ordered_and_settles_abort_none() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let under = |acting: Did, object: &str| NewOwedChange {
            actor: acting,
            ..owed(&tuple(object, "reader", actor(bob())), OwedOp::Add)
        };
        let race = |limit: u64| {
            let outbox = &outbox;
            async move {
                let (first, first_changes) = outbox.begin().await;
                let (second, second_changes) = outbox.begin().await;
                let tag = limit.to_string();
                first_changes
                    .with_node_limit(limit)
                    .stage(under(alice(), &format!("one-{tag}")))
                    .await
                    .unwrap();
                second_changes
                    .with_node_limit(limit)
                    .stage(under(mallory(), &format!("two-{tag}")))
                    .await
                    .unwrap();
                first.commit().await.unwrap();
                second.commit().await
            }
        };
        let far = VALIDATED_WITHIN_OF_NODE_LIMIT + 10;
        assert!(
            race(far).await.is_ok(),
            "far from the bound nothing conflicts"
        );
        assert!(race(4).await.err().is_some_and(is_conflict));

        // Three are owed now. A settle while a stager near the bound is open.
        let settled = outbox.queue().await[0];
        let (txn, changes) = outbox.begin().await;
        changes
            .with_node_limit(4)
            .stage(under(carol(), "three"))
            .await
            .unwrap();
        assert!(outbox.settle(settled).await);
        txn.commit().await.unwrap();

        // And the bound still refuses at its limit.
        let (txn, changes) = outbox.begin().await;
        let changes = changes.with_node_limit(3);
        assert!(matches!(
            changes.stage(under(carol(), "four")).await,
            Err(acp::Error::OwedChangesPerNode {
                unsettled: 3,
                limit: 3
            })
        ));
        txn.discard().unwrap();
    })
    .unwrap()
}

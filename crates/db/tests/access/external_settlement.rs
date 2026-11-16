//! Submitting owed relationship changes to an external ACP provider: in
//! sequence order from the head of the queue, a document's owed registration
//! first, one tuple's changes one behind the other, and each change settled,
//! parked or left by what the provider answers.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use acp::{DocumentACP, OwedOp, OwedScope, OwedSettlement, OwedState, MAX_FAILED_SUBMISSIONS};
use identity::Did;
use query::QueryExecutor;
use storage::RegolithStore;

use super::external_registration::{alice, bob, mallory, RemoteAcp, MANAGER, POLICY, RESOURCE};
use super::external_staging::{carol, Change, Staging};

fn add(actor: Did, doc: &str, subject: Did) -> Change {
    Change::new(actor, doc, OwedOp::Add, subject)
}

fn delete(actor: Did, doc: &str, subject: Did) -> Change {
    Change::new(actor, doc, OwedOp::Delete, subject)
}

fn settled(count: u64) -> OwedSettlement {
    OwedSettlement {
        settled: count,
        ..OwedSettlement::default()
    }
}

impl Staging {
    async fn pass(&self) -> OwedSettlement {
        self.gate
            .settle_owed_changes_once(OwedScope::All)
            .await
            .unwrap()
    }

    /// The states of the committed queue, in sequence order.
    async fn states(&self) -> Vec<OwedState> {
        self.queue()
            .await
            .into_iter()
            .map(|change| change.state)
            .collect()
    }
}

#[test]
fn owed_changes_are_submitted_in_sequence_order_and_settled() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let first = staging.create(alice(), "first", None).await;
        let second = staging.create(alice(), "second", None).await;

        staging.commit_change(add(alice(), &second, bob())).await;
        staging.commit_change(add(alice(), &first, bob())).await;
        staging.commit_change(add(alice(), &first, carol())).await;
        staging.commit_change(delete(alice(), &second, bob())).await;
        assert!(staging.remote.take_writes().is_empty());

        assert_eq!(staging.pass().await, settled(4));
        assert_eq!(
            staging.remote.take_writes(),
            [
                (true, second.clone(), bob().to_string()),
                (true, first.clone(), bob().to_string()),
                (true, first.clone(), carol().to_string()),
                (false, second.clone(), bob().to_string()),
            ]
        );
        assert!(staging.queue().await.is_empty());
        assert!(staging.allowed(&bob(), &first).await);
        assert!(staging.allowed(&carol(), &first).await);
        assert!(!staging.allowed(&bob(), &second).await);
        assert_eq!(staging.pass().await, OwedSettlement::default());
    })
    .unwrap()
}

#[test]
fn a_documents_owed_registration_is_settled_before_its_changes() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let registered = staging.create(alice(), "registered", None).await;

        staging.remote.down.store(true, Ordering::SeqCst);
        let handle = staging.begin().await;
        let fresh = staging.create(alice(), "fresh", Some(&handle)).await;
        staging
            .stage(&handle, add(alice(), &fresh, bob()))
            .await
            .unwrap();
        staging
            .stage(&handle, add(alice(), &registered, bob()))
            .await
            .unwrap();
        let _ = staging.node.runner.commit_txn(&handle).await;
        staging.remote.down.store(false, Ordering::SeqCst);
        assert_eq!(staging.remote.owner(&fresh), None);

        // While the registration cannot be settled, the document's changes
        // wait and the others go on.
        staging
            .remote
            .refuse_registration
            .store(true, Ordering::SeqCst);
        assert_eq!(
            staging.pass().await,
            OwedSettlement {
                settled: 1,
                left: 1,
                ..OwedSettlement::default()
            }
        );
        assert_eq!(
            staging.remote.take_writes(),
            [(true, registered.clone(), bob().to_string())]
        );
        assert_eq!(staging.remote.owner(&fresh), None);

        staging
            .remote
            .refuse_registration
            .store(false, Ordering::SeqCst);
        assert_eq!(staging.pass().await, settled(1));
        assert_eq!(staging.remote.owner(&fresh), Some(alice()));
        assert_eq!(
            staging.remote.take_writes(),
            [(true, fresh.clone(), bob().to_string())]
        );
        assert!(staging.allowed(&bob(), &fresh).await);
    })
    .unwrap()
}

#[test]
fn a_checked_change_the_provider_refuses_holds_its_tuple_and_nothing_else() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .on_provider(alice(), add(alice(), &doc, bob()))
            .await;
        let mut manager = add(alice(), &doc, mallory());
        manager.relation = MANAGER;
        staging.on_provider(alice(), manager.clone()).await;

        // Mallory manages when the change is staged, and no longer when it
        // is submitted.
        let refused = staging.commit_change(add(mallory(), &doc, bob())).await;
        assert!(refused.authority_checked);
        manager.op = OwedOp::Delete;
        staging.on_provider(alice(), manager).await;
        let held = staging.commit_change(delete(alice(), &doc, bob())).await;
        staging.commit_change(add(alice(), &doc, carol())).await;

        assert_eq!(
            staging.pass().await,
            OwedSettlement {
                settled: 1,
                refused: 1,
                left: 1,
                ..OwedSettlement::default()
            }
        );
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), carol().to_string())]
        );
        let queue = staging.queue().await;
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].seq, refused.seq);
        assert!(matches!(&queue[0].state, OwedState::Refused { reason } if !reason.is_empty()));
        assert_eq!(queue[0].attempts, 1);
        assert_eq!(
            queue[0].last_error.as_deref().map(str::is_empty),
            Some(false)
        );
        assert_eq!(queue[1].seq, held.seq);
        assert_eq!(queue[1].state, OwedState::Staged);
        assert_eq!(queue[1].attempts, 0);
        // The delete behind it is not on the provider, and denies all the same.
        assert!(staging.remote.holds(&doc, &bob()));
        assert!(!staging.allowed(&bob(), &doc).await);

        // A parked change is not submitted again.
        assert_eq!(
            staging.pass().await,
            OwedSettlement {
                left: 1,
                ..OwedSettlement::default()
            }
        );
        assert!(staging.remote.take_writes().is_empty());
        assert_eq!(staging.queue().await.len(), 2);
    })
    .unwrap()
}

#[test]
fn an_unchecked_change_the_provider_refuses_is_discarded_and_holds_nothing() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .on_provider(alice(), add(alice(), &doc, bob()))
            .await;

        // Staged while nobody could check it: Mallory manages nothing.
        staging.remote.down.store(true, Ordering::SeqCst);
        let unchecked = staging.commit_change(add(mallory(), &doc, bob())).await;
        assert!(!unchecked.authority_checked);
        staging.remote.down.store(false, Ordering::SeqCst);
        staging.commit_change(delete(alice(), &doc, bob())).await;

        // The owner's revocation behind it reaches the provider in the same
        // pass, and nothing of Mallory's stays on the books.
        assert_eq!(
            staging.pass().await,
            OwedSettlement {
                settled: 1,
                discarded: 1,
                ..OwedSettlement::default()
            }
        );
        assert_eq!(
            staging.remote.take_writes(),
            [(false, doc.clone(), bob().to_string())]
        );
        assert!(staging.queue().await.is_empty());
        assert!(!staging.remote.holds(&doc, &bob()));
        let txn = datastore::BasicTxn::new(staging.store.as_ref(), 0, true)
            .await
            .unwrap();
        let changes = staging
            .gate
            .owed_changes(&txn.acpstore(), acp::StagedCount::new());
        assert_eq!(changes.unsettled().await.unwrap(), 0);
        assert_eq!(changes.unsettled_by(&mallory()).await.unwrap(), 0);
        assert_eq!(changes.unsettled_by(&alice()).await.unwrap(), 0);
        drop(changes);
        txn.discard().unwrap();
    })
    .unwrap()
}

#[test]
fn an_answer_that_is_not_classified_parks_only_after_it_repeats() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        let failing = staging.commit_change(add(alice(), &doc, bob())).await;
        staging.commit_change(delete(alice(), &doc, bob())).await;
        let other = staging.create(alice(), "other", None).await;
        staging.commit_change(add(alice(), &other, bob())).await;
        // The pass does not stop at a change the provider answers with an
        // error: it goes on and submits the other document's change too.
        staging.remote.garble(Some("code 13"));
        assert_eq!(
            staging.pass().await,
            OwedSettlement {
                failed: 2,
                left: 1,
                ..OwedSettlement::default()
            }
        );
        staging.remote.garble(None);
        let queue = staging.queue().await;
        assert_eq!(queue[0].seq, failing.seq);
        assert_eq!(queue[0].state, OwedState::Staged);
        assert_eq!(queue[0].attempts, 1);
        assert_eq!(
            queue[0].last_error.as_deref(),
            Some("storage error: code 13")
        );

        // An outage is not an answer, and another answer starts the count
        // again.
        staging.remote.garble(Some("code 13"));
        for _ in 2..MAX_FAILED_SUBMISSIONS {
            staging.remote.down.store(true, Ordering::SeqCst);
            assert!(staging.pass().await.provider_unavailable);
            staging.remote.down.store(false, Ordering::SeqCst);
            assert_eq!(staging.pass().await.failed, 2);
        }
        assert_eq!(
            staging.queue().await[0].attempts,
            MAX_FAILED_SUBMISSIONS - 1
        );
        staging.remote.garble(Some("code 14"));
        assert_eq!(staging.pass().await.failed, 2);
        assert_eq!(staging.queue().await[0].attempts, 1);

        // The same answer, enough times in a row, is the provider's refusal.
        for _ in 2..MAX_FAILED_SUBMISSIONS {
            assert_eq!(staging.pass().await.failed, 2);
        }
        let parked = staging.pass().await;
        assert_eq!((parked.refused, parked.failed), (2, 0));
        let queue = staging.queue().await;
        assert!(matches!(
            &queue[0].state,
            OwedState::Refused { reason } if reason.contains("code 14")
        ));
        assert_eq!(queue[0].attempts, MAX_FAILED_SUBMISSIONS);
        assert_eq!(
            queue[1].state,
            OwedState::Staged,
            "held behind the parked one"
        );
        assert_eq!(staging.pass().await, left_only(1));
    })
    .unwrap()
}

/// A pass that submits nothing and leaves `left` changes behind parked ones.
fn left_only(left: u64) -> OwedSettlement {
    OwedSettlement {
        left,
        ..OwedSettlement::default()
    }
}

#[test]
fn a_pass_for_one_transaction_submits_what_is_owed_on_its_objects_only() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let first = staging.create(alice(), "first", None).await;
        let second = staging.create(alice(), "second", None).await;
        let by = |txn: u64, change: Change| Change {
            txn: Some(txn),
            ..change
        };
        staging
            .commit_change(by(1, add(alice(), &first, bob())))
            .await;
        staging
            .commit_change(by(1, add(alice(), &second, bob())))
            .await;
        staging
            .commit_change(by(2, add(alice(), &second, carol())))
            .await;

        let pass = |scope| {
            let gate = &staging.gate;
            async move { gate.settle_owed_changes_once(scope).await.unwrap() }
        };
        assert_eq!(
            pass(OwedScope::Transaction(3)).await,
            OwedSettlement::default()
        );
        // The second transaction's object, with what the first owed on it.
        assert_eq!(pass(OwedScope::Transaction(2)).await, settled(2));
        assert_eq!(
            staging.remote.take_writes(),
            [
                (true, second.clone(), bob().to_string()),
                (true, second.clone(), carol().to_string()),
            ]
        );
        assert_eq!(staging.queue().await.len(), 1);
        assert_eq!(pass(OwedScope::All).await, settled(1));
        assert!(staging.remote.holds(&first, &bob()));
    })
    .unwrap()
}

#[test]
fn a_change_whose_authority_lapsed_waits_and_holds_its_tuple() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.remote.credentials.store(true, Ordering::SeqCst);
        staging.remote.carry_delegation(&alice(), "short");
        let waiting = staging.commit_change(add(alice(), &doc, bob())).await;
        staging.remote.hold_key(&alice());
        staging.commit_change(delete(alice(), &doc, bob())).await;
        staging.commit_change(add(alice(), &doc, carol())).await;

        staging.remote.expire("short");
        assert_eq!(
            staging.pass().await,
            OwedSettlement {
                settled: 1,
                awaiting_authority: 1,
                left: 1,
                ..OwedSettlement::default()
            }
        );
        let queue = staging.queue().await;
        assert_eq!(queue[0].seq, waiting.seq);
        assert_eq!(queue[0].state, OwedState::AwaitingAuthority);
        assert_eq!(queue[0].attempts, 1);
        assert_eq!(queue[1].state, OwedState::Staged);
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), carol().to_string())]
        );

        // A key the node no longer holds parks its change the same way.
        staging.remote.drop_key(&alice());
        staging
            .commit_change_with_key(add(alice(), &doc, mallory()))
            .await;
        assert_eq!(staging.pass().await.awaiting_authority, 1);
        assert!(staging.remote.take_writes().is_empty());
    })
    .unwrap()
}

impl Staging {
    /// Stage `change` while the node holds its acting DID's key, which it
    /// has lost again when the change is submitted.
    async fn commit_change_with_key(&self, change: Change) {
        self.remote.hold_key(&change.actor);
        let actor = change.actor.clone();
        self.commit_change(change).await;
        self.remote.drop_key(&actor);
    }
}

#[test]
fn an_unreachable_provider_stops_the_pass_and_leaves_everything_staged() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.commit_change(add(alice(), &doc, bob())).await;
        staging.commit_change(add(alice(), &doc, carol())).await;

        staging.remote.down.store(true, Ordering::SeqCst);
        assert_eq!(
            staging.pass().await,
            OwedSettlement {
                provider_unavailable: true,
                ..OwedSettlement::default()
            }
        );
        assert_eq!(
            staging.states().await,
            [OwedState::Staged, OwedState::Staged]
        );
        assert!(staging
            .queue()
            .await
            .iter()
            .all(|change| change.attempts == 0 && change.last_error.is_none()));

        staging.remote.down.store(false, Ordering::SeqCst);
        assert_eq!(staging.pass().await, settled(2));
        assert!(staging.allowed(&bob(), &doc).await);
        assert!(staging.allowed(&carol(), &doc).await);
    })
    .unwrap()
}

#[test]
fn a_change_the_provider_applied_before_a_crash_is_submitted_again_and_settles() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging
            .on_provider(alice(), add(alice(), &doc, carol()))
            .await;

        // The provider applies the change and the node never learns it, as
        // when it crashes before it deletes the record.
        for change in [add(alice(), &doc, bob()), delete(alice(), &doc, carol())] {
            let write = (
                change.op == OwedOp::Add,
                doc.clone(),
                change.subject.to_string(),
            );
            staging.commit_change(change).await;
            staging.remote.lose_answer.store(true, Ordering::SeqCst);
            assert!(staging.pass().await.provider_unavailable);
            assert_eq!(staging.states().await, [OwedState::Staged]);

            assert_eq!(staging.pass().await, settled(1));
            assert_eq!(staging.remote.take_writes(), [write.clone(), write]);
            assert!(staging.queue().await.is_empty());
        }
        assert!(staging.remote.holds(&doc, &bob()));
        assert!(!staging.remote.holds(&doc, &carol()));
        assert!(staging.allowed(&bob(), &doc).await);
        assert!(!staging.allowed(&carol(), &doc).await);
    })
    .unwrap()
}

/// The window resubmission leaves open: the provider applied the change, the
/// node did not learn it, and by the next submission the acting DID has lost
/// its authority. The provider refuses, and nothing tells that refusal from
/// one of a change that never landed.
#[test]
fn a_resubmission_refused_after_the_change_landed_is_parked_as_refused() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        let mut manager = add(alice(), &doc, bob());
        manager.relation = MANAGER;
        staging.on_provider(alice(), manager.clone()).await;
        staging
            .on_provider(alice(), add(alice(), &doc, carol()))
            .await;
        staging.commit_change(delete(bob(), &doc, carol())).await;

        staging.remote.lose_answer.store(true, Ordering::SeqCst);
        assert!(staging.pass().await.provider_unavailable);
        assert!(!staging.remote.holds(&doc, &carol()), "the delete landed");
        manager.op = OwedOp::Delete;
        staging.on_provider(alice(), manager).await;

        assert_eq!(staging.pass().await.refused, 1);
        assert!(matches!(
            staging.states().await.as_slice(),
            [OwedState::Refused { .. }]
        ));
        // The parked delete keeps denying, which is what its commit meant.
        assert!(!staging.allowed(&carol(), &doc).await);
    })
    .unwrap()
}

#[test]
fn a_change_made_outside_a_transaction_follows_what_is_owed_on_its_tuple() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.commit_change(add(alice(), &doc, bob())).await;
        let other = staging.create(alice(), "other", None).await;
        staging.commit_change(add(alice(), &other, carol())).await;
        let revoke = || {
            let (gate, doc) = (&staging.gate, &doc);
            async move {
                gate.delete_actor_relationship(
                    &alice(),
                    &bob(),
                    POLICY,
                    RESOURCE,
                    doc,
                    "reader",
                    &[],
                )
                .await
            }
        };

        // While the staged grant cannot be submitted, the revocation that
        // follows it is not sent ahead of it.
        staging.remote.lose_answer.store(true, Ordering::SeqCst);
        assert!(matches!(
            revoke().await,
            Err(acp::Error::OwedChangesPending { object, relation })
                if object == doc && relation == "reader"
        ));
        staging.remote.take_writes();

        // Once it can, the pass before the direct change submits what is
        // owed on the document and nothing else.
        assert!(revoke().await.unwrap());
        assert_eq!(
            staging.remote.take_writes(),
            [
                (true, doc.clone(), bob().to_string()),
                (false, doc.clone(), bob().to_string()),
            ]
        );
        assert_eq!(staging.queue().await.len(), 1, "the other document's stays");
        assert!(!staging.allowed(&bob(), &doc).await);

        // With nothing owed on the tuple the provider is asked alone.
        assert!(staging
            .gate
            .add_actor_relationship(&alice(), &bob(), POLICY, RESOURCE, &doc, "reader", &[])
            .await
            .unwrap());
        assert_eq!(
            staging.remote.take_writes(),
            [(true, doc.clone(), bob().to_string())]
        );
    })
    .unwrap()
}

#[test]
fn a_change_made_outside_a_transaction_overtakes_a_parked_one() {
    workers::block_on(async {
        let staging = Staging::new().await;
        let doc = staging.create(alice(), "shared", None).await;
        staging.remote.credentials.store(true, Ordering::SeqCst);
        staging.remote.carry_delegation(&alice(), "short");
        staging.commit_change(add(alice(), &doc, bob())).await;
        staging.remote.expire("short");
        assert_eq!(staging.pass().await.awaiting_authority, 1);

        // Nothing submits the parked grant again by itself, so the direct
        // change does not wait for it.
        assert!(staging
            .gate
            .add_actor_relationship(&alice(), &bob(), POLICY, RESOURCE, &doc, "reader", &[])
            .await
            .unwrap());
        assert_eq!(staging.states().await, [OwedState::AwaitingAuthority]);
    })
    .unwrap()
}

#[test]
fn a_pass_at_the_next_open_settles_what_a_crash_left_owed() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let remote = RemoteAcp::new();
        let doc = {
            let store = Arc::new(RegolithStore::open(dir.path()).unwrap());
            let staging = Staging::on(store, remote.clone(), true).await;
            let doc = staging.create(alice(), "before crash", None).await;
            staging
                .on_provider(alice(), add(alice(), &doc, carol()))
                .await;
            staging.commit_change(add(alice(), &doc, bob())).await;
            staging.commit_change(delete(alice(), &doc, carol())).await;
            staging.node.db.close().await.unwrap();
            doc
        };
        assert!(!remote.holds(&doc, &bob()));

        let store = Arc::new(RegolithStore::open(dir.path()).unwrap());
        let staging = Staging::on(store, remote, false).await;
        assert!(!staging.allowed(&bob(), &doc).await);
        assert!(
            !staging.allowed(&carol(), &doc).await,
            "the owed delete denies across the restart"
        );
        assert_eq!(staging.pass().await, settled(2));
        assert!(staging.allowed(&bob(), &doc).await);
        assert!(!staging.allowed(&carol(), &doc).await);
        assert!(staging.queue().await.is_empty());
    })
    .unwrap()
}

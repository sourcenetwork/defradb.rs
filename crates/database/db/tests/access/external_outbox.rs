//! Relationship changes owed to an external ACP provider: a change is
//! recorded in the transaction that stages it and commits or rolls back with
//! it, and changes of one tuple are ordered as their transactions commit.

use std::sync::Arc;

use acp::outbox::{StagedCount, MAX_OWED_CHANGES_PER_TRANSACTION, MAX_UNSETTLED_OWED_CHANGES};
use acp::{
    ExternalDocumentAcp, NewOwedChange, OwedAuthority, OwedChanges, OwedOp, OwedState, OwedSubject,
    OwedTuple, OwedTupleState,
};
use datastore::BasicTxn;
use identity::Did;
use storage::RegolithStore;

use super::external_registration::{alice, bob, gate, mallory, RemoteAcp, POLICY, RESOURCE};

pub(super) type Gate = Arc<ExternalDocumentAcp<RegolithStore>>;

pub(super) fn actor(did: Did) -> OwedSubject {
    OwedSubject::Actor { did }
}

pub(super) fn tuple(object: &str, relation: &str, subject: OwedSubject) -> OwedTuple {
    OwedTuple {
        policy_id: POLICY.into(),
        resource: RESOURCE.into(),
        object: object.into(),
        relation: relation.into(),
        subject,
    }
}

pub(super) fn owed(tuple: &OwedTuple, op: OwedOp) -> NewOwedChange {
    NewOwedChange {
        txn: None,
        tuple: tuple.clone(),
        op,
        actor: alice(),
        authority: OwedAuthority::NodeKey,
        authority_checked: true,
    }
}

pub(super) struct Outbox {
    pub(super) store: Arc<RegolithStore>,
    pub(super) gate: Gate,
}

impl Outbox {
    pub(super) fn new() -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let gate = gate(&RemoteAcp::new(), store.clone());
        Self { store, gate }
    }

    pub(super) async fn begin(&self) -> (BasicTxn, OwedChanges) {
        let txn = BasicTxn::new(self.store.as_ref(), 0, false).await.unwrap();
        let changes = self.gate.owed_changes(&txn.acpstore(), StagedCount::new());
        (txn, changes)
    }

    /// Stage one change in a transaction of its own and commit it.
    pub(super) async fn commit(&self, tuple: &OwedTuple, op: OwedOp) -> u64 {
        let (txn, changes) = self.begin().await;
        let seq = changes.stage(owed(tuple, op)).await.unwrap().seq;
        txn.commit().await.unwrap();
        seq
    }

    pub(super) async fn settle(&self, seq: u64) -> bool {
        let (txn, changes) = self.begin().await;
        let settled = changes.settle(seq).await.unwrap();
        txn.commit().await.unwrap();
        settled
    }

    /// The sequences of the whole queue, read a page at a time.
    pub(super) async fn queue(&self) -> Vec<u64> {
        let (txn, changes) = self.begin().await;
        let mut seqs = Vec::new();
        loop {
            let page = changes.page(seqs.last().copied()).await.unwrap();
            if page.is_empty() {
                break;
            }
            seqs.extend(page.iter().map(|change| change.seq));
        }
        txn.discard().unwrap();
        seqs
    }

    /// What is committed about one tuple: its state, its object's count and
    /// the node's.
    pub(super) async fn committed(&self, tuple: &OwedTuple) -> (Option<OwedTupleState>, u64, u64) {
        let (txn, changes) = self.begin().await;
        let state = (
            changes.tuple(tuple).await.unwrap(),
            changes
                .unsettled_on(&tuple.policy_id, &tuple.resource, &tuple.object)
                .await
                .unwrap(),
            changes.unsettled().await.unwrap(),
        );
        txn.discard().unwrap();
        state
    }
}

/// A tuple with `unsettled` changes owed from `oldest` to `latest`, `deletes`
/// of them deletes.
fn held(oldest: u64, latest: u64, unsettled: u32, deletes: u32) -> Option<OwedTupleState> {
    Some(OwedTupleState {
        oldest,
        latest,
        unsettled,
        deletes,
    })
}

pub(super) fn is_conflict(error: datastore::Error) -> bool {
    matches!(error, datastore::Error::Storage(error) if error.is_txn_conflict())
}

#[test]
fn a_staged_change_commits_with_its_transaction() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));

        let (txn, changes) = outbox.begin().await;
        let staged = changes
            .stage(NewOwedChange {
                txn: Some(7),
                authority: OwedAuthority::Delegation {
                    token: "header.claims.signature".into(),
                },
                ..owed(&reader, OwedOp::Delete)
            })
            .await
            .unwrap();
        assert_eq!(staged.state, OwedState::Staged);
        assert_eq!((staged.attempts, &staged.last_error), (0, &None));
        assert_eq!(
            changes.get(staged.seq).await.unwrap(),
            Some(staged.clone()),
            "the transaction reads its own change"
        );
        assert_eq!(
            outbox.committed(&reader).await,
            (None, 0, 0),
            "nothing is owed before the commit"
        );
        txn.commit().await.unwrap();

        assert_eq!(
            outbox.committed(&reader).await,
            (held(staged.seq, staged.seq, 1, 1), 1, 1)
        );
        let (txn, changes) = outbox.begin().await;
        assert_eq!(changes.get(staged.seq).await.unwrap(), Some(staged.clone()));
        assert_eq!(changes.page(None).await.unwrap(), vec![staged]);
        txn.discard().unwrap();
    })
    .unwrap()
}

#[test]
fn a_rolled_back_transaction_owes_nothing() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));

        let (txn, changes) = outbox.begin().await;
        let seq = changes
            .stage(owed(&reader, OwedOp::Delete))
            .await
            .unwrap()
            .seq;
        txn.discard().unwrap();
        assert!(changes.stage(owed(&reader, OwedOp::Add)).await.is_err());

        assert_eq!(outbox.committed(&reader).await, (None, 0, 0));
        assert!(outbox.queue().await.is_empty());
        assert!(!outbox.settle(seq).await);
    })
    .unwrap()
}

#[test]
fn a_savepoint_rollback_drops_the_changes_staged_after_it() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let kept = tuple("doc-1", "reader", actor(bob()));
        let dropped = tuple("doc-1", "reader", actor(mallory()));
        let staged = StagedCount::new();
        let mut txn = BasicTxn::new(outbox.store.as_ref(), 0, false)
            .await
            .unwrap();
        let open = |txn: &BasicTxn| outbox.gate.owed_changes(&txn.acpstore(), staged.clone());

        let seq = open(&txn)
            .stage(owed(&kept, OwedOp::Add))
            .await
            .unwrap()
            .seq;
        txn.set_savepoint().unwrap();
        let at_savepoint = staged.get();
        let changes = open(&txn);
        changes.stage(owed(&dropped, OwedOp::Add)).await.unwrap();
        changes.stage(owed(&kept, OwedOp::Delete)).await.unwrap();
        assert_eq!(staged.get(), 3);
        drop(changes);
        txn.rollback_to_savepoint().unwrap();
        staged.restore(at_savepoint);

        let changes = open(&txn);
        assert_eq!(changes.tuple(&kept).await.unwrap(), held(seq, seq, 1, 0));
        assert_eq!(changes.unsettled().await.unwrap(), 1);
        drop(changes);
        txn.commit().await.unwrap();

        assert_eq!(staged.get(), 1);
        assert_eq!(outbox.committed(&kept).await, (held(seq, seq, 1, 0), 1, 1));
        assert_eq!(outbox.committed(&dropped).await.0, None);
        assert_eq!(outbox.queue().await, [seq]);
    })
    .unwrap()
}

#[test]
fn two_transactions_on_one_tuple_conflict_and_the_later_commit_is_the_later_change() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));

        let (first, first_changes) = outbox.begin().await;
        let (second, second_changes) = outbox.begin().await;
        let lost = first_changes
            .stage(owed(&reader, OwedOp::Add))
            .await
            .unwrap()
            .seq;
        let won = second_changes
            .stage(owed(&reader, OwedOp::Delete))
            .await
            .unwrap()
            .seq;
        assert!(lost < won);

        second.commit().await.unwrap();
        assert!(is_conflict(first.commit().await.unwrap_err()));
        assert_eq!(
            outbox.committed(&reader).await,
            (held(won, won, 1, 1), 1, 1)
        );

        let rerun = outbox.commit(&reader, OwedOp::Add).await;
        assert!(
            rerun > won,
            "the attempt that commits later takes the later sequence"
        );
        assert_eq!(
            outbox.committed(&reader).await,
            (held(won, rerun, 2, 1), 2, 2)
        );
        assert_eq!(outbox.queue().await, [won, rerun]);
    })
    .unwrap()
}

#[test]
fn transactions_on_different_objects_do_not_conflict() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let one = tuple("doc-1", "reader", actor(bob()));
        let other = tuple("doc-2", "reader", actor(bob()));

        let (first, first_changes) = outbox.begin().await;
        let (second, second_changes) = outbox.begin().await;
        let first_seq = first_changes
            .stage(owed(&one, OwedOp::Delete))
            .await
            .unwrap()
            .seq;
        let second_seq = second_changes
            .stage(owed(&other, OwedOp::Delete))
            .await
            .unwrap()
            .seq;
        second.commit().await.unwrap();
        first.commit().await.unwrap();

        assert!(
            first_seq < second_seq,
            "across tuples the sequence follows staging, not commit, order"
        );
        assert_eq!(outbox.queue().await, [first_seq, second_seq]);
        assert_eq!(
            outbox.committed(&one).await,
            (held(first_seq, first_seq, 1, 1), 1, 2)
        );
        assert_eq!(
            outbox.committed(&other).await,
            (held(second_seq, second_seq, 1, 1), 1, 2)
        );
    })
    .unwrap()
}

#[test]
fn settling_a_change_leaves_the_later_ones_of_its_tuple_owed() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));
        let writer = tuple("doc-1", "writer", actor(bob()));

        let delete = outbox.commit(&reader, OwedOp::Delete).await;
        let add = outbox.commit(&reader, OwedOp::Add).await;
        let other = outbox.commit(&writer, OwedOp::Delete).await;

        assert!(outbox.settle(delete).await);
        assert_eq!(
            outbox.committed(&reader).await,
            (held(add, add, 1, 0), 2, 2)
        );
        assert_eq!(outbox.queue().await, [add, other]);

        assert!(outbox.settle(add).await);
        assert_eq!(outbox.committed(&reader).await, (None, 1, 1));
        assert_eq!(
            outbox.committed(&writer).await,
            (held(other, other, 1, 1), 1, 1)
        );

        assert!(outbox.settle(other).await);
        assert_eq!(outbox.committed(&writer).await, (None, 0, 0));
        assert!(outbox.queue().await.is_empty());
        assert!(!outbox.settle(other).await, "a settled change is gone");
    })
    .unwrap()
}

#[test]
fn settling_out_of_order_keeps_the_oldest_the_latest_and_the_deletes_true() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));
        let noise = tuple("doc-2", "reader", actor(bob()));

        let first = outbox.commit(&reader, OwedOp::Delete).await;
        outbox.commit(&noise, OwedOp::Add).await;
        let second = outbox.commit(&reader, OwedOp::Add).await;
        let third = outbox.commit(&reader, OwedOp::Delete).await;
        let fourth = outbox.commit(&reader, OwedOp::Add).await;
        assert_eq!(
            outbox.committed(&reader).await,
            (held(first, fourth, 4, 2), 4, 5)
        );

        assert!(outbox.settle(third).await);
        assert_eq!(
            outbox.committed(&reader).await,
            (held(first, fourth, 3, 1), 3, 4),
            "a change between the oldest and the latest moves neither"
        );
        assert!(outbox.settle(fourth).await);
        assert_eq!(
            outbox.committed(&reader).await,
            (held(first, second, 2, 1), 2, 3),
            "the latest left is the one staged before it"
        );
        assert!(outbox.settle(first).await);
        assert_eq!(
            outbox.committed(&reader).await,
            (held(second, second, 1, 0), 1, 2),
            "the oldest left is the one staged after it"
        );
        assert!(outbox.settle(second).await);
        assert_eq!(outbox.committed(&reader).await, (None, 0, 1));
    })
    .unwrap()
}

#[test]
fn the_queue_pages_in_sequence_order() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let mut staged = Vec::new();
        for index in 0..300 {
            let (txn, changes) = outbox.begin().await;
            let tuple = tuple(&format!("doc-{index}"), "reader", actor(bob()));
            let change = NewOwedChange {
                authority: OwedAuthority::Delegation {
                    token: "t".repeat(4096),
                },
                ..owed(&tuple, OwedOp::Add)
            };
            staged.push(changes.stage(change).await.unwrap().seq);
            txn.commit().await.unwrap();
        }
        assert!(staged.windows(2).all(|pair| pair[0] < pair[1]));

        let (txn, changes) = outbox.begin().await;
        let first = changes.page(None).await.unwrap();
        assert!(
            !first.is_empty() && first.len() < staged.len(),
            "a page holds a bounded part of the queue"
        );
        let after = changes.page(Some(staged[10])).await.unwrap();
        assert_eq!(after[0].seq, staged[11]);
        let next: Vec<u64> = changes
            .next(staged[10], 3)
            .await
            .unwrap()
            .iter()
            .map(|change| change.seq)
            .collect();
        assert_eq!(next, staged[11..14], "a look at the next few reads no more");
        assert!(changes.page(Some(u64::MAX)).await.unwrap().is_empty());
        txn.discard().unwrap();

        assert_eq!(outbox.queue().await, staged);
    })
    .unwrap()
}

#[test]
fn a_transaction_stages_no_more_than_its_bound() {
    workers::block_on(async {
        assert_eq!(MAX_OWED_CHANGES_PER_TRANSACTION, 256);
        let outbox = Outbox::new();
        let (txn, changes) = outbox.begin().await;
        for index in 0..MAX_OWED_CHANGES_PER_TRANSACTION {
            let tuple = tuple(&format!("doc-{index}"), "reader", actor(bob()));
            changes.stage(owed(&tuple, OwedOp::Add)).await.unwrap();
        }
        let over = tuple("doc-over", "reader", actor(bob()));
        assert!(matches!(
            changes.clone().stage(owed(&over, OwedOp::Add)).await,
            Err(acp::Error::OwedChangesPerTransaction { limit: 256 })
        ));
        txn.commit().await.unwrap();

        let staged = StagedCount::new();
        let txn = BasicTxn::new(outbox.store.as_ref(), 0, false)
            .await
            .unwrap();
        for index in 0..2 {
            let tuple = tuple(&format!("later-{index}"), "reader", actor(bob()));
            let handle = outbox.gate.owed_changes(&txn.acpstore(), staged.clone());
            handle.stage(owed(&tuple, OwedOp::Add)).await.unwrap();
        }
        assert_eq!(
            staged.get(),
            2,
            "the handles of one transaction share its count"
        );
        txn.discard().unwrap();

        assert_eq!(outbox.queue().await.len(), 256);
        assert_eq!(outbox.committed(&over).await, (None, 0, 256));
    })
    .unwrap()
}

#[test]
fn a_node_holds_no_more_unsettled_changes_than_its_bound() {
    workers::block_on(async {
        assert_eq!(MAX_UNSETTLED_OWED_CHANGES, 10_000);
        let outbox = Outbox::new();
        let tuples: Vec<OwedTuple> = (0..4)
            .map(|index| tuple(&format!("doc-{index}"), "reader", actor(bob())))
            .collect();

        let (txn, changes) = outbox.begin().await;
        let changes = changes.with_node_limit(3);
        let first = changes
            .stage(owed(&tuples[0], OwedOp::Add))
            .await
            .unwrap()
            .seq;
        changes.stage(owed(&tuples[1], OwedOp::Add)).await.unwrap();
        txn.commit().await.unwrap();

        let (txn, changes) = outbox.begin().await;
        let changes = changes.with_node_limit(3);
        changes.stage(owed(&tuples[2], OwedOp::Add)).await.unwrap();
        assert!(
            matches!(
                changes.stage(owed(&tuples[3], OwedOp::Add)).await,
                Err(acp::Error::OwedChangesPerNode {
                    unsettled: 3,
                    limit: 3
                })
            ),
            "the bound counts what the transaction staged itself"
        );
        txn.commit().await.unwrap();
        assert_eq!(outbox.committed(&tuples[3]).await, (None, 0, 3));

        assert!(outbox.settle(first).await);
        let (txn, changes) = outbox.begin().await;
        changes
            .with_node_limit(3)
            .stage(owed(&tuples[3], OwedOp::Add))
            .await
            .unwrap();
        txn.commit().await.unwrap();
        assert_eq!(outbox.queue().await.len(), 3);
    })
    .unwrap()
}

/// A change whose stored error leaves no room in its record is still
/// marked in flight: the error is cut as the mark is added, as it is when a
/// state is recorded, so the change is never stuck in front of its tuple.
#[test]
fn a_change_with_a_long_error_can_still_be_marked_in_flight() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));
        let seq = outbox.commit(&reader, OwedOp::Add).await;

        // An error that brings the unmarked record to the cap exactly.
        let (txn, changes) = outbox.begin().await;
        let staged = changes.get(seq).await.unwrap().expect("the change");
        let unmarked = acp::OwedChange {
            last_error: Some(String::new()),
            ..staged
        };
        let room = acp::outbox::MAX_OWED_CHANGE_BYTES - unmarked.encode().unwrap().len();
        let long = "e".repeat(room);
        assert!(changes
            .mark(seq, OwedState::Staged, 1, Some(long.clone()))
            .await
            .unwrap());
        txn.commit().await.unwrap();

        let (txn, changes) = outbox.begin().await;
        let id = "x".repeat(acp::outbox::MAX_REQUEST_ID_BYTES);
        let flight = acp::InFlight::new(id.clone(), Some(id), Some(u64::MAX)).unwrap();
        assert!(changes.mark_in_flight(&[seq], &flight).await.unwrap());
        let marked = changes.get(seq).await.unwrap().expect("the change");
        assert_eq!(marked.in_flight, Some(flight));
        assert!(marked.last_error.unwrap().len() < long.len());
        txn.commit().await.unwrap();
    })
    .unwrap()
}

#[test]
fn a_change_over_the_record_cap_is_refused_and_leaves_nothing() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));
        let (txn, changes) = outbox.begin().await;
        let large = NewOwedChange {
            authority: OwedAuthority::Delegation {
                token: "t".repeat(acp::outbox::MAX_OWED_CHANGE_BYTES),
            },
            ..owed(&reader, OwedOp::Add)
        };
        assert!(matches!(
            changes.stage(large).await,
            Err(acp::Error::OwedChangeTooLarge { .. })
        ));
        txn.commit().await.unwrap();
        assert_eq!(outbox.committed(&reader).await, (None, 0, 0));
        assert!(outbox.queue().await.is_empty());
    })
    .unwrap()
}

/// A change is admitted only with room left to park it or mark it in flight:
/// the largest record staging takes is still written as refused and as
/// awaiting authority, with the provider's words whole up to the reserved
/// length, and marked in flight with the longest request names after such
/// an answer; a record one byte larger is refused when it is staged.
#[test]
fn the_largest_change_staged_can_still_be_parked() {
    workers::block_on(async {
        let outbox = Outbox::new();
        let reader = tuple("doc-1", "reader", actor(bob()));
        let with_token = |length: usize| NewOwedChange {
            authority: OwedAuthority::Delegation {
                token: "t".repeat(length),
            },
            ..owed(&reader, OwedOp::Add)
        };
        let words = "x".repeat(acp::outbox::RESERVED_REASON_BYTES);
        let refused = OwedState::Refused {
            reason: words.clone(),
        };

        // What the largest later state adds to a record, measured on a
        // small one.
        let (txn, changes) = outbox.begin().await;
        let small = changes.stage(with_token(0)).await.unwrap();
        txn.discard().unwrap();
        let id = "x".repeat(acp::outbox::MAX_REQUEST_ID_BYTES);
        let longest = acp::InFlight::new(id.clone(), Some(id), Some(u64::MAX)).unwrap();
        let marked = acp::OwedChange {
            attempts: u32::MAX,
            last_error: Some(words.clone()),
            in_flight: Some(longest.clone()),
            alone: true,
            ..small.clone()
        };
        let parked = acp::OwedChange {
            state: refused.clone(),
            attempts: u32::MAX,
            last_error: Some(words.clone()),
            alone: true,
            ..small
        };
        let later = parked
            .encode()
            .unwrap()
            .len()
            .max(marked.encode().unwrap().len());
        let largest = acp::outbox::MAX_OWED_CHANGE_BYTES - later;

        let (txn, changes) = outbox.begin().await;
        assert!(matches!(
            changes.stage(with_token(largest + 1)).await,
            Err(acp::Error::OwedChangeTooLarge { .. })
        ));
        let staged = changes.stage(with_token(largest)).await.unwrap();
        assert_eq!(staged.seq.to_string().len(), small.seq.to_string().len());
        txn.commit().await.unwrap();

        let (txn, changes) = outbox.begin().await;
        assert!(changes
            .mark(staged.seq, OwedState::Staged, u32::MAX, Some(words.clone()))
            .await
            .unwrap());
        assert!(changes
            .mark_in_flight(&[staged.seq], &longest)
            .await
            .unwrap());
        let in_flight = changes.get(staged.seq).await.unwrap().expect("the change");
        assert_eq!(in_flight.in_flight, Some(longest));
        txn.discard().unwrap();

        for state in [refused, OwedState::AwaitingAuthority] {
            let (txn, changes) = outbox.begin().await;
            assert!(changes
                .mark(staged.seq, state.clone(), u32::MAX, Some(words.clone()))
                .await
                .unwrap());
            let marked = changes.get(staged.seq).await.unwrap().expect("the change");
            assert_eq!(marked.state, state, "the words are kept whole");
            assert_eq!(marked.last_error.as_deref(), Some(words.as_str()));
            txn.commit().await.unwrap();
        }
    })
    .unwrap()
}

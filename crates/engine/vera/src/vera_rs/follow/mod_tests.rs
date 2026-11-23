//! Checking a term against the journal, with a deployment the test plays.

use super::effect::{Lost, LostSubject};
use super::fixtures::*;
use super::*;
use crate::SubjectRef;

fn journal(directory: &std::path::Path, confirmations: &[Confirmation]) -> Journal {
    let mut journal = Journal::open(directory, 0).expect("journal");
    for confirmation in confirmations {
        journal.record(confirmation).expect("record");
    }
    journal
}

fn reader() -> SubjectRef {
    SubjectRef::Actor(READER.into())
}

fn lost_grant(add: bool) -> Lost {
    Lost::Relationship {
        add,
        policy_id: POLICY.into(),
        resource: "users".into(),
        object: "bae-1".into(),
        relation: "reader".into(),
        subject: LostSubject::Actor { did: READER.into() },
        token: token(OWNER),
    }
}

fn lost_registration(object: &str) -> Lost {
    Lost::Registration {
        policy_id: POLICY.into(),
        resource: "users".into(),
        object: object.into(),
        token: token(OWNER),
    }
}

fn ignore(_: alloy_primitives::B256, _: bool) -> Result<(), ProviderError> {
    Ok(())
}

fn expected_checked(owed: Vec<Lost>) -> Reconciled {
    Reconciled::Checked {
        report: TakeoverReport {
            owed,
            ..TakeoverReport::default()
        },
    }
}

fn block_on<F>(future: F) -> F::Output
where
    F: std::future::Future,
{
    futures::executor::block_on(future)
}

#[test]
fn a_term_that_voided_nothing_of_this_node_is_checked_at_once() {
    let directory = tempfile::tempdir().unwrap();
    let reader = reader();
    let confirmations = [
        confirmed(wire(0, "bae-1", register()), 5),
        confirmed(wire(1, "bae-1", grant(&reader)), 6),
    ];
    let mut journal = journal(directory.path(), &confirmations);
    let deployment = Deployment::holding(&confirmations);
    let follow = Follow::from_journal(&journal);
    assert!(follow.admit(1).is_err(), "an unchecked term admits nothing");

    let checked = block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).unwrap();
    assert_eq!(checked, expected_checked(Vec::new()));
    assert!(deployment.replayed().is_empty());
    follow.update(&journal);
    assert_eq!(follow.epoch(), 1);
    follow.admit(1).expect("a checked term admits");
}

#[test]
fn a_missing_registration_and_grant_are_sent_again_and_what_is_refused_is_owed_in_order() {
    let directory = tempfile::tempdir().unwrap();
    let reader = reader();
    let kept = confirmed(wire(0, "bae-0", register()), 5);
    let registered = confirmed(wire(1, "bae-1", register()), 6);
    let granted = confirmed(wire(2, "bae-1", grant(&reader)), 7);
    let mut journal = journal(
        directory.path(),
        &[kept.clone(), registered.clone(), granted.clone()],
    );
    // The takeover's cut is block 5: the registration and the grant are void,
    // and the registration's delegation has expired since.
    let deployment = Deployment::holding([&kept]).refuse(&registered);

    let checked = block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).unwrap();
    // Both are sent again in their order; the grant executes, but it comes
    // after a refused registration of its object, so it is owed again too.
    assert_eq!(deployment.replayed(), vec![1, 2]);
    assert_eq!(
        checked,
        Reconciled::Checked {
            report: TakeoverReport {
                owed: vec![lost_registration("bae-1"), lost_grant(true)],
                ..TakeoverReport::default()
            },
        }
    );
    // The journal keeps both at the height they were executed at again, and
    // what is owed until it is handed over.
    assert_eq!(entries(&journal)[&2].height, 102);
    assert_eq!(journal.undelivered().len(), 2);
    let follow = Follow::from_journal(&journal);
    assert!(
        follow.admit(1).is_err(),
        "nothing is decided before they are owed"
    );
    journal.delivered().unwrap();
    follow.update(&journal);
    follow
        .admit(1)
        .expect("owed, they deny or wait in the outbox");
}

#[test]
fn a_revocation_on_a_void_block_that_is_refused_again_is_owed_as_a_delete() {
    let directory = tempfile::tempdir().unwrap();
    let reader = reader();
    let granted = confirmed(wire(0, "bae-1", grant(&reader)), 5);
    let revoked = confirmed(wire(1, "bae-1", revoke(&reader)), 6);
    let mut journal = journal(directory.path(), &[granted.clone(), revoked.clone()]);
    let deployment = Deployment::holding([&granted]).refuse(&revoked);

    let checked = block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).unwrap();
    assert_eq!(
        checked,
        Reconciled::Checked {
            report: TakeoverReport {
                owed: vec![lost_grant(false)],
                ..TakeoverReport::default()
            },
        }
    );
}

#[test]
fn a_replay_cut_short_still_owes_what_it_learnt_and_taints_its_object() {
    let directory = tempfile::tempdir().unwrap();
    let reader = reader();
    let kept = confirmed(wire(0, "bae-0", register()), 5);
    let revoked = confirmed(wire(1, "bae-1", revoke(&reader)), 6);
    let granted = confirmed(wire(2, "bae-1", grant(&reader)), 7);
    let mut journal = journal(directory.path(), &[kept.clone(), revoked.clone(), granted]);
    let mut deployment = Deployment::holding([&kept]).refuse(&revoked);
    deployment.unreachable_after = Some(1);

    assert!(block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).is_err());
    assert_eq!(journal.handled_term(), 0, "the term is still unchecked");

    // Run again, the revocation is held (its replay was refused, and spent
    // its sequence); it is sent again all the same, and the grant after it,
    // which executes, is owed behind it.
    deployment.unreachable_after = None;
    let checked = block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).unwrap();
    assert_eq!(
        checked,
        Reconciled::Checked {
            report: TakeoverReport {
                owed: vec![lost_grant(false), lost_grant(true)],
                ..TakeoverReport::default()
            },
        }
    );
    assert_eq!(journal.replaying(), None);
}

#[test]
fn a_refused_revocation_whose_answer_was_lost_is_still_owed_after_a_restart() {
    let directory = tempfile::tempdir().unwrap();
    let reader = reader();
    let kept = confirmed(wire(0, "bae-0", register()), 5);
    let revoked = confirmed(wire(1, "bae-1", revoke(&reader)), 6);
    let mut journal = journal(directory.path(), &[kept.clone(), revoked.clone()]);
    // The revocation is sent again and refused, and the node stops before
    // it learns so: the deployment holds the refused request, so asking it
    // shows nothing missing.
    let mut deployment = Deployment::holding([&kept]).refuse(&revoked);
    deployment.answer_lost_at = Some(0);
    assert!(block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).is_err());
    drop(journal);

    let mut journal = Journal::open(directory.path(), 0).unwrap();
    deployment.answer_lost_at = None;
    let checked = block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).unwrap();
    assert_eq!(
        checked,
        Reconciled::Checked {
            report: TakeoverReport {
                owed: vec![lost_grant(false)],
                ..TakeoverReport::default()
            },
        },
        "the revocation is owed although the deployment holds its refused replay"
    );
}

#[test]
fn a_replay_an_earlier_term_left_unfinished_is_sent_again_under_the_next() {
    let directory = tempfile::tempdir().unwrap();
    let reader = reader();
    let kept = confirmed(wire(0, "bae-0", register()), 5);
    let revoked = confirmed(wire(1, "bae-1", revoke(&reader)), 6);
    let mut journal = journal(directory.path(), &[kept.clone(), revoked.clone()]);
    let mut deployment = Deployment::holding([&kept]).refuse(&revoked);
    deployment.answer_lost_at = Some(0);
    assert!(block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).is_err());

    // A second takeover keeps everything: still the revocation is owed.
    deployment.answer_lost_at = None;
    let checked = block_on(reconcile(&mut journal, &deployment, 2, &mut ignore)).unwrap();
    assert_eq!(
        checked,
        Reconciled::Checked {
            report: TakeoverReport {
                owed: vec![lost_grant(false)],
                ..TakeoverReport::default()
            },
        }
    );
}

#[test]
fn a_read_that_a_term_check_overtook_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(directory.path(), 0).unwrap();
    let follow = Follow::from_journal(&journal);
    let epoch = follow.admit(0).unwrap();
    // While the read is on its way a term is certified and checked.
    block_on(reconcile(
        &mut journal,
        &Deployment::default(),
        1,
        &mut ignore,
    ))
    .unwrap();
    follow.update(&journal);
    follow.admit(1).expect("the checked term admits new reads");
    assert!(
        follow.still(epoch, 1).is_err(),
        "an answer from before the check is not served"
    );
    follow.still(1, 1).expect("one read at the checked term is");
}

#[test]
fn a_checked_term_is_published_with_what_it_leaves_to_owe() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(directory.path(), 0).unwrap();
    let follow = Follow::from_journal(&journal);
    journal.handled(1, vec![lost_grant(false)]).unwrap();
    follow.update(&journal);
    assert!(
        follow.admit(1).is_err(),
        "the owed revocation blocks admission"
    );
}

fn full_journal(directory: &std::path::Path) -> (Journal, Vec<Confirmation>) {
    let mut journal = Journal::open(directory, 0).unwrap().keeping(3);
    let mut confirmations = Vec::new();
    for sequence in 0..5 {
        let confirmation = confirmed(wire(sequence, "bae-1", register()), 5 + sequence);
        journal.record(&confirmation).unwrap();
        confirmations.push(confirmation);
    }
    (journal, confirmations)
}

#[test]
fn a_void_older_than_the_journal_fails_closed_for_good() {
    let directory = tempfile::tempdir().unwrap();
    let (mut journal, confirmations) = full_journal(directory.path());
    assert!(matches!(
        journal.floor(),
        Some(Floor::Pruned { sequence: 1, .. })
    ));
    // The takeover's cut is below everything the journal still holds and
    // below the newest entry it pruned.
    let deployment = Deployment::holding(&confirmations[..1]);

    let Reconciled::Closed { reason, .. } =
        block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).unwrap()
    else {
        panic!("a void the journal cannot vouch for must fail closed");
    };
    assert!(reason.contains("sequence 1"), "{reason}");
    assert!(deployment.replayed().is_empty(), "nothing is sent");
    let follow = Follow::from_journal(&journal);
    assert!(
        matches!(follow.admit(0), Err(ProviderError::Unavailable(message)) if message == reason)
    );

    drop(journal);
    let journal = Journal::open(directory.path(), 0).unwrap();
    assert_eq!(
        journal.closed(),
        Some(reason.as_str()),
        "closed across a restart"
    );
}

#[test]
fn a_void_within_the_journal_after_pruning_is_checked() {
    let directory = tempfile::tempdir().unwrap();
    let (mut journal, confirmations) = full_journal(directory.path());
    let held = confirmations.len() - 1;
    let deployment = Deployment::holding(&confirmations[..held]);

    let checked = block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).unwrap();
    assert_eq!(
        checked,
        Reconciled::Checked {
            report: TakeoverReport {
                applied: vec![report::Applied::Change(lost_registration("bae-1"))],
                owed: Vec::new(),
                unrestorable: Vec::new(),
            },
        }
    );
    assert_eq!(deployment.replayed(), vec![held as u64]);
}

#[test]
fn a_journal_begun_after_the_worker_signed_cannot_vouch_for_a_void_reaching_back() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(directory.path(), 3).unwrap();
    let confirmation = confirmed(wire(3, "bae-1", register()), 9);
    journal.record(&confirmation).unwrap();
    let deployment = Deployment::default();

    let outcome = block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).unwrap();
    assert!(matches!(
        outcome,
        Reconciled::Closed { ref reason, .. } if reason.contains("below 3")
    ));
}

#[test]
fn a_void_archive_that_is_refused_again_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let kept = confirmed(wire(0, "bae-1", register()), 5);
    let archived = confirmed(archive(1, "bae-1"), 6);
    let mut journal = journal(directory.path(), &[kept.clone(), archived.clone()]);
    let deployment = Deployment::holding([&kept]).refuse(&archived);

    let outcome = block_on(reconcile(&mut journal, &deployment, 1, &mut ignore)).unwrap();
    assert!(matches!(
        outcome,
        Reconciled::Closed { ref reason, .. } if reason.contains("archive")
    ));
}

#[test]
fn every_term_is_a_new_epoch_for_cached_decisions() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(directory.path(), 0).unwrap();
    let follow = Follow::from_journal(&journal);
    assert_eq!(follow.epoch(), 0);
    for term in [1, 3] {
        block_on(reconcile(
            &mut journal,
            &Deployment::default(),
            term,
            &mut ignore,
        ))
        .unwrap();
        follow.update(&journal);
        assert_eq!(follow.epoch(), term);
    }
    assert!(follow.admit(4).is_err());
}

#[test]
fn whatever_names_a_request_sent_again_learns_what_it_came_to() {
    let directory = tempfile::tempdir().unwrap();
    let reader = reader();
    let kept = confirmed(wire(0, "bae-0", register()), 5);
    let granted = confirmed(wire(1, "bae-1", grant(&reader)), 6);
    let revoked = confirmed(wire(2, "bae-1", revoke(&reader)), 7);
    let mut journal = journal(
        directory.path(),
        &[kept.clone(), granted.clone(), revoked.clone()],
    );
    let deployment = Deployment::holding([&kept]).refuse(&revoked);
    let mut told = Vec::new();
    let mut revise = |submission, executed| {
        told.push((submission, executed));
        Ok(())
    };
    block_on(reconcile(&mut journal, &deployment, 1, &mut revise)).unwrap();
    assert_eq!(
        told,
        vec![(granted.submission, true), (revoked.submission, false)]
    );
}

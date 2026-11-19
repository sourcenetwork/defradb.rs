//! The confirmation journal on disk.

use super::super::effect::Lost;
use super::super::fixtures::{confirmed, wire};
use super::*;
use crate::PolicyAction;

fn confirmation(sequence: u64) -> Confirmation {
    confirmed(
        wire(sequence, "bae-1", PolicyAction::RegisterObject),
        10 + sequence,
    )
}

#[test]
fn entries_survive_a_reopen_and_a_new_worker_starts_with_no_floor() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(directory.path(), 0).unwrap();
    assert_eq!(journal.floor(), None);
    for sequence in 0..3 {
        journal.record(&confirmation(sequence)).unwrap();
    }
    // Confirming one again keeps it once, at its new height.
    journal
        .record(&Confirmation {
            height: 99,
            ..confirmation(1)
        })
        .unwrap();
    drop(journal);

    let journal = Journal::open(directory.path(), 3).unwrap();
    assert_eq!(journal.sequences().collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(journal.entry(1).unwrap().height, 99);
    assert_eq!(journal.entry(2).unwrap(), confirmation(2));
    assert_eq!(journal.floor(), None, "the floor is set only when created");
}

#[test]
fn pruning_keeps_the_newest_entries_and_records_the_floor_before_it_removes() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(directory.path(), 0).unwrap().keeping(2);
    for sequence in 0..4 {
        journal.record(&confirmation(sequence)).unwrap();
    }
    assert_eq!(journal.sequences().collect::<Vec<_>>(), vec![2, 3]);
    assert_eq!(
        journal.floor(),
        Some(&Floor::Pruned {
            sequence: 1,
            submission: confirmation(1).submission
        })
    );
    drop(journal);
    let journal = Journal::open(directory.path(), 4).unwrap();
    assert_eq!(journal.sequences().collect::<Vec<_>>(), vec![2, 3]);
    assert!(matches!(
        journal.floor(),
        Some(Floor::Pruned { sequence: 1, .. })
    ));
}

#[test]
fn a_journal_begun_for_a_worker_that_signed_before_cannot_vouch_for_those_requests() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open(directory.path(), 7).unwrap();
    assert_eq!(journal.floor(), Some(&Floor::Unjournaled { before: 7 }));
}

#[test]
fn a_gap_in_the_sequences_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(directory.path(), 0).unwrap();
    journal.record(&confirmation(0)).unwrap();
    assert!(matches!(
        journal.record(&confirmation(2)),
        Err(JournalError::Broken(_))
    ));
}

#[test]
fn the_term_checked_what_is_owed_and_a_closure_are_kept() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(directory.path(), 0).unwrap();
    let owed = vec![Lost::Registration {
        policy_id: "p".into(),
        resource: "users".into(),
        object: "bae-1".into(),
        token: "t".into(),
    }];
    journal.handled(2, owed.clone()).unwrap();
    journal.handled(1, owed.clone()).unwrap();
    drop(journal);
    let mut journal = Journal::open(directory.path(), 0).unwrap();
    assert_eq!(
        journal.handled_term(),
        2,
        "the term checked never goes back"
    );
    assert_eq!(journal.undelivered(), owed.as_slice());
    journal.delivered().unwrap();
    journal.close("why".into()).unwrap();
    drop(journal);
    let journal = Journal::open(directory.path(), 0).unwrap();
    assert!(journal.undelivered().is_empty());
    assert_eq!(journal.closed(), Some("why"));
}

#[cfg(unix)]
#[test]
fn a_failed_write_leaves_what_the_journal_reports_as_it_was() {
    use std::os::unix::fs::PermissionsExt as _;
    let directory = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(directory.path(), 0).unwrap();
    let owed = vec![Lost::Registration {
        policy_id: "p".into(),
        resource: "users".into(),
        object: "bae-1".into(),
        token: "t".into(),
    }];
    journal.handled(1, owed.clone()).unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let delivered = journal.delivered();
    let handled = journal.handled(2, Vec::new());
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(delivered.is_err() && handled.is_err());
    assert_eq!(journal.undelivered(), owed.as_slice(), "still owed");
    assert_eq!(journal.handled_term(), 1);
}

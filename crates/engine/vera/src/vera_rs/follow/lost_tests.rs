//! Which replays leave a change to owe again.

use super::super::effect::LostSubject;
use super::*;

fn change(object: &str, add: bool) -> Effect {
    Effect::Owed(Lost::Relationship {
        add,
        policy_id: "p".into(),
        resource: "users".into(),
        object: object.into(),
        relation: "reader".into(),
        subject: LostSubject::Wildcard,
        token: "t".into(),
    })
}

#[test]
fn a_refused_replay_taints_the_later_ones_on_its_object_only() {
    let mut losses = Losses::default();
    let mut tainted = RapidHashSet::new();
    lost_effects(&mut losses, &mut tainted, change("a", true), true);
    lost_effects(&mut losses, &mut tainted, change("a", false), false);
    lost_effects(&mut losses, &mut tainted, change("b", true), true);
    lost_effects(&mut losses, &mut tainted, change("a", true), true);
    lost_effects(&mut losses, &mut tainted, Effect::Other, false);
    let owed: Vec<_> = losses
        .owed
        .iter()
        .map(|lost| match lost {
            Lost::Relationship { object, add, .. } => (object.as_str(), *add),
            Lost::Registration { .. } | Lost::Archive { .. } | Lost::Transfer { .. } => {
                unreachable!()
            }
        })
        .collect();
    assert_eq!(owed, vec![("a", false), ("a", true)]);
    assert_eq!(losses.other, 1);
    assert!(losses.unrestorable.is_empty());
}

#[test]
fn what_was_owed_before_a_replay_stopped_still_taints() {
    let mut losses = Losses::default();
    let Effect::Owed(earlier) = change("a", false) else {
        unreachable!()
    };
    let mut tainted = tainted_by(std::slice::from_ref(&earlier));
    lost_effects(&mut losses, &mut tainted, change("a", true), true);
    assert_eq!(losses.owed.len(), 1);
}

fn creation() -> Effect {
    Effect::PolicyCreation {
        policy_id: "p".into(),
        creator: "did:key:creator".into(),
    }
}

fn replayed(executed: bool, exists: bool) -> Replayed {
    Replayed {
        executed,
        exists,
        height: 1,
    }
}

#[test]
fn a_policy_creation_refused_because_the_policy_is_there_is_held_not_lost() {
    assert!(held_again(&creation(), replayed(true, false)));
    assert!(held_again(&creation(), replayed(false, true)));
    assert!(!held_again(&creation(), replayed(false, false)));
    assert!(
        !held_again(&change("a", true), replayed(false, true)),
        "a registration or grant refused as existing is still owed"
    );

    let mut losses = Losses::default();
    let mut tainted = RapidHashSet::new();
    let held = held_again(&creation(), replayed(false, true));
    lost_effects(&mut losses, &mut tainted, creation(), held);
    let lost = held_again(&creation(), replayed(false, false));
    lost_effects(&mut losses, &mut tainted, creation(), lost);
    assert_eq!(
        losses.applied,
        vec![Applied::PolicyCreation {
            policy_id: "p".into(),
            creator: "did:key:creator".into(),
        }]
    );
    assert!(matches!(
        losses.unrestorable.as_slice(),
        [Unrestorable::PolicyCreation { policy_id, creator, .. }]
            if policy_id == "p" && creator == "did:key:creator"
    ));
}

fn archive(object: &str) -> Lost {
    Lost::Archive {
        policy_id: "p".into(),
        resource: "users".into(),
        object: object.into(),
        token: "t".into(),
    }
}

#[test]
fn a_refused_archive_is_owed_again_and_nothing_is_unrestorable() {
    let mut losses = Losses::default();
    let mut tainted = RapidHashSet::new();
    lost_effects(&mut losses, &mut tainted, Effect::Owed(archive("a")), false);
    assert_eq!(losses.owed, vec![archive("a")]);
    assert!(losses.unrestorable.is_empty());
    assert_eq!(losses.other, 0);
}

#[test]
fn a_refused_batch_owes_its_archives_and_counts_what_it_could_not_read() {
    let mut losses = Losses::default();
    let mut tainted = RapidHashSet::new();
    let batch = Effect::Batched(vec![archive("a"), archive("b")], 1);
    lost_effects(&mut losses, &mut tainted, batch, false);
    assert_eq!(losses.owed, vec![archive("a"), archive("b")]);
    assert_eq!(losses.other, 1);
    assert!(losses.unrestorable.is_empty());
}

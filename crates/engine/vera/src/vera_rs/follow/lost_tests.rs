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
            Lost::Registration { .. } => unreachable!(),
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

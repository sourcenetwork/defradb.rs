//! Which replays leave a change to be owed again.
//!
//! A request sent again exactly as signed is executed again or refused (its
//! delegation may have expired since). A refused one is owed again with
//! fresh authority. So is every later request on the same object, even one
//! that executed again: owed again, the refused change is sent after it, and
//! sent once more behind the refused one it keeps its place
//! (`proofs/tla/FollowDeployment.tla`, Taint).

use rapidhash::{HashSetExt, RapidHashSet};

use super::effect::{Effect, Lost};

/// An object, as `(policy, resource, object)`.
pub(crate) type ObjectKey = (String, String, String);

/// What the replays of a term's lost submissions leave to put right.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Losses {
    /// Changes to owe again, in the order they were confirmed.
    pub owed: Vec<Lost>,
    /// Other requests that were refused, such as a policy creation.
    pub other: usize,
}

/// The objects `owed` is on: what was owed again before a replay stopped
/// taints the replays that come after it.
pub(crate) fn tainted_by(owed: &[Lost]) -> RapidHashSet<ObjectKey> {
    let mut objects = RapidHashSet::with_capacity(owed.len());
    for lost in owed {
        let (policy, resource, object) = lost.object();
        objects.insert((policy.into(), resource.into(), object.into()));
    }
    objects
}

/// Adds the replay of `effect`, executed again when `executed`, to `losses`.
///
/// A batch keeps its commands together: one refusal taints every object it
/// carried, so a later refused archive cannot overtake an earlier lost one.
pub(crate) fn lost_effects(
    losses: &mut Losses,
    tainted: &mut RapidHashSet<ObjectKey>,
    effect: Effect,
    executed: bool,
) {
    match effect {
        Effect::Batched(effects, other) => {
            if !executed {
                losses.other += other;
            }
            for effect in effects {
                lost_effects(losses, tainted, Effect::Owed(effect), executed);
            }
        }
        Effect::Owed(_) | Effect::Other => {
            let object = effect.object();
            let lost = !executed || object.as_ref().is_some_and(|key| tainted.contains(key));
            if !lost {
                return;
            }
            if let Some(key) = object {
                tainted.insert(key);
            }
            match effect {
                Effect::Owed(change) => losses.owed.push(change),
                Effect::Other => losses.other += 1,
                Effect::Batched(_, _) => unreachable!(),
            }
        }
    }
}

#[cfg(test)]
#[path = "lost_tests.rs"]
mod tests;

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
use super::report::{Applied, Unrestorable};

/// An object, as `(policy, resource, object)`.
pub(crate) type ObjectKey = (String, String, String);

/// What the replays of a term's lost submissions leave to put right.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Losses {
    /// Submissions sent again and held by the deployment, in order.
    pub applied: Vec<Applied>,
    /// Changes to owe again, in the order they were confirmed.
    pub owed: Vec<Lost>,
    /// Submissions whose effect cannot be restored here.
    pub unrestorable: Vec<Unrestorable>,
    /// Refused submissions whose effect cannot be named.
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
pub(crate) fn lost_effects(
    losses: &mut Losses,
    tainted: &mut RapidHashSet<ObjectKey>,
    effect: Effect,
    executed: bool,
) {
    let object = effect.object();
    let lost = !executed || object.as_ref().is_some_and(|key| tainted.contains(key));
    if !lost {
        match effect {
            Effect::Owed(change) => losses.applied.push(Applied::Change(change)),
            Effect::PolicyCreation { policy_id } => {
                losses.applied.push(Applied::PolicyCreation { policy_id });
            }
            Effect::Archive {
                policy_id,
                resource,
                object,
            } => losses.applied.push(Applied::Archive {
                policy_id,
                resource,
                object,
            }),
            Effect::Other => {}
        }
        return;
    }
    if let Some(key) = object.clone() {
        tainted.insert(key);
    }
    match effect {
        Effect::Owed(change) => losses.owed.push(change),
        Effect::Archive {
            policy_id,
            resource,
            object,
        } => {
            losses.unrestorable.push(Unrestorable::Archive {
                policy_id,
                resource,
                object,
                reason: "the Vera deployment switched its active device and voided the archive; \
                         it was refused when sent again"
                    .into(),
            });
        }
        Effect::PolicyCreation { policy_id } => {
            losses.unrestorable.push(Unrestorable::PolicyCreation {
                policy_id,
                reason: "the Vera deployment switched its active device and voided the policy; \
                         it was refused when sent again"
                    .into(),
            });
        }
        Effect::Other => losses.other += 1,
    }
}

#[cfg(test)]
#[path = "lost_tests.rs"]
mod tests;

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
use super::Replayed;

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

/// Whether the deployment holds what `effect` did after its replay came to
/// `replayed`: it executed again, or it is a policy creation refused because
/// the policy is there. A policy's ID derives from its definition, creator
/// and nonce, so the policy found under it is the one the request created.
pub(crate) fn held_again(effect: &Effect, replayed: Replayed) -> bool {
    replayed.executed || (replayed.exists && matches!(effect, Effect::PolicyCreation { .. }))
}

/// Adds the replay of `effect`, held again when `executed`
/// ([`held_again`]), to `losses`.
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
            Effect::PolicyCreation { policy_id, creator } => {
                losses
                    .applied
                    .push(Applied::PolicyCreation { policy_id, creator });
            }
            Effect::Archive {
                policy_id,
                resource,
                object,
                actor,
            } => losses.applied.push(Applied::Archive {
                policy_id,
                resource,
                object,
                actor,
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
            actor,
        } => {
            losses.unrestorable.push(Unrestorable::Archive {
                policy_id,
                resource,
                object,
                actor,
                reason: "the Vera deployment switched its active device and voided the archive; \
                         it was refused when sent again"
                    .into(),
            });
        }
        Effect::PolicyCreation { policy_id, creator } => {
            losses.unrestorable.push(Unrestorable::PolicyCreation {
                policy_id,
                creator,
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

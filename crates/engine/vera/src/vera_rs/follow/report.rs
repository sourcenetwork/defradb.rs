//! What checking one takeover did to each submission it replayed.

use super::effect::Lost;

/// A submission the deployment held again after it was sent again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Applied {
    /// A registration, relationship change or archive.
    Change(Lost),
    /// A policy creation, named by its derived policy ID and its creator. A
    /// creation refused because the policy is there is held too.
    PolicyCreation { policy_id: String, creator: String },
}

/// A submission whose effect the node cannot put back. A refused archive is
/// not one: the outbox owes it again like any change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Unrestorable {
    /// A policy creation refused when sent again: its documents cannot be
    /// read until the policy is created again.
    PolicyCreation {
        policy_id: String,
        creator: String,
        reason: String,
    },
}

/// Every replayed submission, in the place it holds: the deployment holds it
/// again, the node owes it again, or it could not be restored.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct TakeoverReport {
    pub applied: Vec<Applied>,
    pub owed: Vec<Lost>,
    pub unrestorable: Vec<Unrestorable>,
}

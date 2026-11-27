//! A policy command the node sends an external ACP provider for an actor.
//!
//! An external provider (Vera) takes a command from the node only under the
//! actor's authority. A delegation an actor binds to one request names
//! exactly one command, so whoever asks what lets the node send something
//! says which command it is about to send.

use crate::outbox::{OwedOp, OwedTuple};

/// One command sent to an external provider as an actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderCommand<'a> {
    /// Register `object` with the actor as its owner, together with the
    /// relationships it carries through its `@policy(relation:)` fields.
    RegisterObject {
        policy_id: &'a str,
        resource: &'a str,
        object: &'a str,
        relationships: &'a [crate::DerivedRelationship],
    },
    /// Add or delete one relationship.
    Relationship { tuple: &'a OwedTuple, op: OwedOp },
}

//! A policy command as it is sent to a provider under a bearer token.
//!
//! A delegation bound to one request names exactly one such command, so a
//! provider that takes those delegations is told which command a token is
//! about to be sent with
//! ([`VeraProvider::caller_of_request_delegation`](crate::VeraProvider::caller_of_request_delegation)).

use crate::provider::SubjectRef;

/// One command on one object of a policy.
#[derive(Clone, Copy)]
pub struct PolicyCommand<'a> {
    pub policy_id: &'a str,
    pub resource: &'a str,
    pub object_id: &'a str,
    pub action: PolicyAction<'a>,
}

/// What a [`PolicyCommand`] does to its object.
#[derive(Clone, Copy)]
pub enum PolicyAction<'a> {
    /// Register the object with the token's actor as its owner.
    RegisterObject,
    /// Add one relationship.
    SetRelationship {
        relation: &'a str,
        subject: PolicySubject<'a>,
    },
    /// Delete one relationship.
    DeleteRelationship {
        relation: &'a str,
        subject: PolicySubject<'a>,
    },
}

/// The subject of a relationship command, in the two forms a provider is
/// sent one.
#[derive(Clone, Copy)]
pub enum PolicySubject<'a> {
    /// One actor, or every actor.
    Actor(&'a SubjectRef),
    /// A userset or another object, as [`zanzibar::encode_subject`] writes
    /// it.
    Structured {
        kind: u8,
        resource: &'a str,
        object_id: &'a str,
        relation: &'a str,
    },
}

//! A policy command as it is sent to a provider under a bearer token.
//!
//! A delegation bound to one request names exactly one such command, by its
//! digest ([`request_digest`](crate::request_digest)), so a provider that
//! takes those delegations is told which command a token is about to be sent
//! with.

use crate::SubjectRef;

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
    /// Register the object with the token's actor as its owner, together
    /// with the relationships it carries through its `@policy(relation:)`
    /// fields: vera.rs checks the collection's `create` over them and stores
    /// them with the owner. Never empty: see [`PolicyAction::register`].
    RegisterObjectWithRelationships {
        relationships: &'a [acp::DerivedRelationship],
    },
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
    /// Archive the object: every relationship on it goes and its owner no
    /// longer counts. Only its owner may.
    ArchiveObject,
    /// Give the object to another owner, named by its DID: the owner tuple
    /// names it from then on, the previous owner keeps nothing by
    /// ownership, and every other relationship stays. Only whoever manages
    /// the owner relation, normally the owner alone, may.
    TransferObject { new_owner: &'a str },
}

impl<'a> PolicyAction<'a> {
    /// The registration of an object carrying `relationships`: a plain
    /// [`PolicyAction::RegisterObject`] when it carries none.
    pub fn register(relationships: &'a [acp::DerivedRelationship]) -> Self {
        if relationships.is_empty() {
            Self::RegisterObject
        } else {
            Self::RegisterObjectWithRelationships { relationships }
        }
    }
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

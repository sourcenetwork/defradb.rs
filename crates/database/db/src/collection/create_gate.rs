//! What a create in a collection gated by its policy is checked against.
//!
//! A policy resource may restrict who adds objects to it (the `create` of its
//! `collection` section). The check runs on relationships derived from the new
//! document's `@policy(relation:)` fields, which are not stored until the
//! document is registered with them. This module derives them; it neither
//! checks nor registers.

use document::{Document, NormalValue};
use schema::CollectionVersion;

pub use acp::DerivedRelationship;

/// The create gate of a document: whether its collection restricts creates,
/// and the relationships the check (and the registration after it) uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateGate {
    /// The collection is governed and its policy resource has a `create`
    /// other than `"*"`.
    pub gated: bool,
    /// One per bound field with a non-null value, in field order.
    pub derived: Vec<DerivedRelationship>,
    /// The resource's `create`, decided over `derived`, when `gated`.
    pub create: Option<String>,
}

/// Why a create gate could not be derived: the policy does not match the
/// collection it was given for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CreateGateError {
    /// The collection names another policy than the one given.
    #[error("the policy of collection {collection} is {expected}, not {given}")]
    WrongPolicy {
        collection: String,
        expected: String,
        given: String,
    },
    /// The policy has no resource the collection names.
    #[error("resource {resource} does not exist on policy {policy_id}")]
    MissingResource { policy_id: String, resource: String },
    /// A bound relation does not admit exactly one object type, which the
    /// collection add checks; a policy that changed under its ID would.
    #[error("relation {relation} of resource {resource} does not admit one related resource")]
    UnboundRelation { resource: String, relation: String },
}

/// The create gate of `doc` in collection `version` under `policy`, the
/// collection's policy. Without a policy on either side (a collection with
/// none, a node with no DAC) nothing is gated and nothing derived.
///
/// Only a governed collection is gated, because only a governed identity
/// carries its bindings to other nodes. The target resource of each derived
/// relationship is the one the bound relation admits, which the collection
/// add checked is the related collection's resource.
pub fn create_gate(
    version: &CollectionVersion,
    doc: &Document,
    policy: Option<&acp::Policy>,
) -> Result<CreateGate, CreateGateError> {
    create_gate_of(
        version,
        |field| match doc.get(field) {
            Some(NormalValue::String(value)) => Some(value.clone()),
            _ => None,
        },
        policy,
    )
}

/// [`create_gate`] of a document whose field values `field` reads: the
/// value of a bound `_xID` field as a string, `None` when it is null or
/// absent, and an empty ID is none. A replicated document's values are read
/// from what was merged.
pub fn create_gate_of(
    version: &CollectionVersion,
    field: impl Fn(&str) -> Option<String>,
    policy: Option<&acp::Policy>,
) -> Result<CreateGate, CreateGateError> {
    let (Some(described), Some(policy)) = (version.policy.as_ref(), policy) else {
        return Ok(CreateGate::default());
    };
    if described.id != policy.id {
        return Err(CreateGateError::WrongPolicy {
            collection: version.name.clone(),
            expected: described.id.clone(),
            given: policy.id.clone(),
        });
    }
    let resource = policy
        .get_resource(&described.resource_name)
        .ok_or_else(|| CreateGateError::MissingResource {
            policy_id: policy.id.clone(),
            resource: described.resource_name.clone(),
        })?;
    let derived = query::derived_relationships::derived_relationships(
        version,
        |binding| field(&binding.id_field.name),
        |binding| {
            resource
                .get_relation(binding.policy_relation)
                .and_then(|relation| match &relation.subject_restriction {
                    Some(acp::SubjectRestriction::EntitySet { resource, relation })
                        if relation.is_empty() =>
                    {
                        Some(resource.clone())
                    }
                    _ => None,
                })
                .ok_or_else(|| CreateGateError::UnboundRelation {
                    resource: described.resource_name.clone(),
                    relation: binding.policy_relation.to_owned(),
                })
        },
    )?;
    let gated = version.governance_root.is_some() && resource.is_create_gated();
    Ok(CreateGate {
        gated,
        derived,
        create: gated.then(|| resource.collection_create.clone()).flatten(),
    })
}

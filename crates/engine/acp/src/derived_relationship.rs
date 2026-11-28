//! Relationships a new document carries through its `@policy(relation:)`
//! fields, registered with it.
//!
//! A policy resource may restrict who adds objects to it (the `create` of its
//! `collection` section). An external provider checks `create` over these
//! relationships and stores them with the owner in one execution; the local
//! engine ignores them.

use serde::{Deserialize, Serialize};

/// `<resource>:<doc>#<relation>@<target_resource>:<target_doc_id>`, on the
/// document it is registered with.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DerivedRelationship {
    /// The policy relation the field is bound to.
    pub relation: String,
    /// The resource of the related document's collection.
    pub target_resource: String,
    /// The related document's ID, the bound `_xID` field's value.
    pub target_doc_id: String,
}

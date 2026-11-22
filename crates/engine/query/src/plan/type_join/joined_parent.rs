//! Metadata needed to reapply a relation joined into a wrapped parent plan.

use crate::document::DocumentMapping;

/// A one-to-one relation the wrapped parent plan has already joined.
#[derive(Debug, Clone)]
pub struct JoinedParentRelation {
    /// The relation field on the wrapped parent's raw document.
    pub relation_field_name: String,
    /// The wrapped parent's foreign key to the related document.
    pub fk_field_name: String,
    /// The collection holding the related document.
    pub child_collection: String,
    /// The mapping used to render the related document.
    pub child_mapping: DocumentMapping,
}

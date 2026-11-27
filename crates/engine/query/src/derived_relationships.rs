//! The relationships a new document carries through its collection's
//! `@policy(relation:)` fields: one per bound field with a document ID, in
//! field order. The node registers the document with them, a delegation
//! bound to its registration names them, and `create` is decided over them,
//! so every path derives them here.

use schema::{CollectionVersion, PolicyBinding};

/// The relationships `version`'s bound fields carry, where `value` reads a
/// bound field's document ID by field name (`None` when null or absent) and
/// `target` names the resource of the document a binding points to. An
/// empty ID is no ID.
pub fn derived_relationships<E, V, T>(
    version: &CollectionVersion,
    value: V,
    mut target: T,
) -> Result<Vec<acp::DerivedRelationship>, E>
where
    V: Fn(&PolicyBinding<'_>) -> Option<String>,
    T: FnMut(&PolicyBinding<'_>) -> Result<String, E>,
{
    let mut derived = Vec::new();
    for binding in version.policy_bindings() {
        let Some(target_doc_id) = value(&binding).filter(|id| !id.is_empty()) else {
            continue;
        };
        derived.push(acp::DerivedRelationship {
            relation: binding.policy_relation.to_owned(),
            target_resource: target(&binding)?,
            target_doc_id,
        });
    }
    Ok(derived)
}

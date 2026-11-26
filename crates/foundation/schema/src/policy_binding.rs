//! Fields bound to a policy relation with `@policy(relation:)`.
//!
//! A binding says that the document a single-object relation field points
//! to is the subject of a policy relation on the new document, so acp can
//! decide who may create it (`book:<docId>#author@author:<_authorID>`). The
//! binding lives on the relation's `_xID` field, which holds that document ID,
//! as `@immutable` does.

use crate::{CollectionVersion, FieldDescription, FieldKind, SchemaError};

/// One field bound to a policy relation.
#[derive(Debug, Clone, Copy)]
pub struct PolicyBinding<'a> {
    /// The `_xID` field holding the related document's ID.
    pub id_field: &'a FieldDescription,
    /// The relation field `x`, whose kind names the related collection.
    pub relation_field: Option<&'a FieldDescription>,
    /// The policy relation the field is bound to.
    pub policy_relation: &'a str,
}

impl PolicyBinding<'_> {
    /// The name a schema author wrote the binding on: the relation field,
    /// or the `_xID` field when it stands alone.
    pub fn declared_name(&self) -> &str {
        self.relation_field
            .map_or(self.id_field.name.as_str(), |field| field.name.as_str())
    }
}

/// A binding whose related collection was found.
#[derive(Debug, Clone, Copy)]
pub struct BoundRelation<'a> {
    pub binding: PolicyBinding<'a>,
    /// The collection the bound field points to.
    pub target: &'a CollectionVersion,
}

impl CollectionVersion {
    /// Every field of this version bound to a policy relation.
    pub fn policy_bindings(&self) -> impl Iterator<Item = PolicyBinding<'_>> {
        self.fields.iter().filter_map(move |field| {
            let policy_relation = field.policy_relation.as_deref()?;
            let relation_field = field
                .name
                .strip_prefix('_')
                .and_then(|name| name.strip_suffix("ID"))
                .and_then(|name| self.field_by_name(name))
                .filter(|object| object.kind.is_relation() && !object.kind.is_array());
            Some(PolicyBinding {
                id_field: field,
                relation_field,
                policy_relation,
            })
        })
    }
}

/// The collection a relation field of `host` points to, among `collections`.
pub fn related_collection<'a>(
    host: &'a CollectionVersion,
    kind: &FieldKind,
    collections: &'a [CollectionVersion],
) -> Option<&'a CollectionVersion> {
    match kind {
        FieldKind::Named { name, .. } => collections.iter().find(|col| col.name == *name),
        FieldKind::Relation { collection_id, .. } => collections.iter().find(|col| {
            col.collection_id == *collection_id
                || col.version_id == *collection_id
                || col.name == *collection_id
        }),
        FieldKind::SelfRef { relative_id, .. } if relative_id.is_empty() => Some(host),
        FieldKind::SelfRef { relative_id, .. } => {
            let host_set = host.collection_set.as_ref()?;
            collections.iter().find(|col| {
                col.collection_set.as_ref().is_some_and(|set| {
                    set.collection_set_id == host_set.collection_set_id
                        && set.relative_id.to_string() == *relative_id
                })
            })
        }
        _ => None,
    }
}

/// Check what a collection's bindings need from the schema alone, and find
/// each one's related collection among `collections` (which may include
/// `collection` itself).
///
/// A binding needs a governed collection, since only a governed identity
/// carries it to other nodes; the primary side of the relation, the side
/// that stores `_xID`; an immutable field, so no update can change what a
/// create was checked against; a policy on the collection; and the same
/// policy on the related collection. What the policy itself says is the
/// caller's to check against the returned relations.
pub fn check_policy_bindings<'a>(
    collection: &'a CollectionVersion,
    collections: &'a [CollectionVersion],
) -> Result<Vec<BoundRelation<'a>>, SchemaError> {
    let mut bound = Vec::new();
    for binding in collection.policy_bindings() {
        let refuse = |reason: String| SchemaError::InvalidPolicyRelation {
            field_name: format!("{}.{}", collection.name, binding.declared_name()),
            reason,
        };
        if collection.governance_root.is_none() {
            return Err(refuse(
                "the collection must be governed (@governed(root:))".into(),
            ));
        }
        let relation_field = binding
            .relation_field
            .ok_or_else(|| refuse("the field is not a single-object relation".into()))?;
        if !binding.id_field.is_primary {
            return Err(refuse(
                "the binding must be on the primary side of the relation, which stores the related document ID".into(),
            ));
        }
        if !binding.id_field.immutable {
            return Err(refuse("the field must be @immutable".into()));
        }
        let Some(policy) = collection.policy.as_ref() else {
            return Err(refuse("the collection must have a @policy".into()));
        };
        let target = related_collection(collection, &relation_field.kind, collections)
            .ok_or_else(|| refuse("the related collection was not found".into()))?;
        if target.policy.as_ref().map(|p| p.id.as_str()) != Some(policy.id.as_str()) {
            return Err(refuse(format!(
                "the related collection {} must have the same policy ({})",
                target.name, policy.id
            )));
        }
        bound.push(BoundRelation { binding, target });
    }
    Ok(bound)
}

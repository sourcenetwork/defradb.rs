//! Validators for the fields an update adds to an existing collection.

use rapidhash::RapidHashSet;

use crate::{CollectionVersion, FieldDescription, FieldKind, ScalarKind};

use super::DefinitionState;

/// The version an updated collection replaces: the same version, or else the
/// collection's active one.
fn old_version<'state>(
    new_col: &CollectionVersion,
    old_state: &'state DefinitionState,
) -> Option<&'state CollectionVersion> {
    old_state
        .collections_by_id
        .get(&new_col.version_id)
        .or_else(|| {
            old_state
                .active_by_collection_id
                .get(&new_col.collection_id)
        })
}

/// Each field of an updated collection that its old version does not have,
/// with the collection it belongs to.
fn new_fields<'state>(
    new_state: &'state DefinitionState,
    old_state: &DefinitionState,
) -> Vec<(&'state CollectionVersion, &'state FieldDescription)> {
    let mut added = Vec::new();
    for new_col in &new_state.collections {
        let Some(old_col) = old_version(new_col, old_state) else {
            continue;
        };
        let old_ids: RapidHashSet<&str> = old_col.fields.iter().map(|f| f.id.as_str()).collect();
        added.extend(
            new_col
                .fields
                .iter()
                .filter(|f| f.id.is_empty() || !old_ids.contains(f.id.as_str()))
                .map(|f| (new_col, f)),
        );
    }
    added
}

/// Whether `field` is the `_<name>ID` field of a relation field in `col`.
fn is_relation_id_field(col: &CollectionVersion, field: &FieldDescription) -> bool {
    field
        .name
        .strip_prefix('_')
        .and_then(|rest| rest.strip_suffix("ID"))
        .is_some_and(|base| {
            col.fields.iter().any(|f| {
                f.name == base
                    && matches!(
                        f.kind,
                        FieldKind::Named { .. }
                            | FieldKind::Relation { .. }
                            | FieldKind::SelfRef { .. }
                    )
            })
        })
}

/// Go's name for a kind in a relational ID field error.
fn relation_id_kind_name(kind: &FieldKind) -> String {
    let FieldKind::Scalar(scalar) = kind else {
        return format!("{kind:?}");
    };
    match scalar {
        ScalarKind::None => "0",
        ScalarKind::DocID => "ID",
        ScalarKind::NillableBool => "Boolean",
        ScalarKind::Bool => "Boolean!",
        ScalarKind::NillableInt => "Integer",
        ScalarKind::Int => "Integer!",
        ScalarKind::NillableFloat64 => "Float",
        ScalarKind::Float64 => "Float!",
        ScalarKind::NillableFloat32 => "Float32",
        ScalarKind::Float32 => "Float32!",
        ScalarKind::NillableDateTime => "DateTime",
        ScalarKind::DateTime => "DateTime!",
        ScalarKind::NillableString => "String",
        ScalarKind::String => "String!",
        ScalarKind::NillableBlob => "Blob",
        ScalarKind::Blob => "Blob!",
        ScalarKind::NillableJson => "JSON",
        ScalarKind::Json => "JSON!",
    }
    .to_string()
}

/// Matches Go's relational ID field kind check: an added `_<name>ID` field of
/// a relation holds a DocID.
pub(super) fn validate_new_relation_id_field_kind(
    new_state: &DefinitionState,
    old_state: &DefinitionState,
) -> Vec<String> {
    new_fields(new_state, old_state)
        .into_iter()
        .filter(|(col, field)| is_relation_id_field(col, field))
        .filter(|(_, field)| !matches!(field.kind, FieldKind::Scalar(ScalarKind::DocID)))
        .map(|(_, field)| {
            format!(
                "relational id field of invalid kind. Field: {}, Expected: ID, Actual: {}",
                field.name,
                relation_id_kind_name(&field.kind)
            )
        })
        .collect()
}

/// Matches Go: a field added to an existing collection must be nillable, as
/// its documents have no value for it.
pub(super) fn validate_new_field_nillable(
    new_state: &DefinitionState,
    old_state: &DefinitionState,
) -> Vec<String> {
    new_fields(new_state, old_state)
        .into_iter()
        .filter(|(col, field)| !is_relation_id_field(col, field) && !field.kind.is_nillable())
        .map(|_| {
            "adding a non-nillable field to an existing collection is not supported".to_string()
        })
        .collect()
}

/// Matches Go: an added field names a kind.
pub(super) fn validate_new_field_kind_given(
    new_state: &DefinitionState,
    old_state: &DefinitionState,
) -> Vec<String> {
    new_fields(new_state, old_state)
        .into_iter()
        .filter(|(col, field)| {
            !is_relation_id_field(col, field)
                && matches!(field.kind, FieldKind::Scalar(ScalarKind::None))
                && field.name != "_docID"
        })
        .map(|_| {
            format!(
                "no type found for given name. Type: {}",
                ScalarKind::None as u8
            )
        })
        .collect()
}

/// Matches Go: an added field has a name.
pub(super) fn validate_new_field_named(
    new_state: &DefinitionState,
    old_state: &DefinitionState,
) -> Vec<String> {
    new_fields(new_state, old_state)
        .into_iter()
        .filter(|(col, field)| !is_relation_id_field(col, field) && field.name.is_empty())
        .map(|_| r#"Names must match /^[_a-zA-Z][_a-zA-Z0-9]*$/ but "" does not."#.to_string())
        .collect()
}

//! Schema patching operations for DefraDB.
//!
//! A patch is a JSON Patch (RFC 6902) document over every collection version,
//! held as a map keyed by version ID. Its paths start with the collection they
//! change (`/Users/Fields/-`), by name or version ID. One patch may change
//! several collections. The whole patch has its GQL values and names replaced
//! with stored ones, is applied in one go, and the resulting versions are
//! validated together before each changed collection is stored.

mod added;
mod in_place;
mod patch_slop;
mod store;
mod substitute_slop;
mod validate;
mod version;

use crate::collection::Collection;
use crate::error::{Error, Result};
use crate::txn::DbTxn;
use rapidhash::{RapidHashMap, RapidHashSet};
use schema::{CollectionSource, CollectionVersion};
use serde_json::Value;
use storage::corekv::{Key, OnCommit, Store};
use storage::keys::systemstore::{CollectionKey, CollectionNameKey, CollectionVersionKey};
use tracing::instrument;

/// A collection version the patch changes.
pub(crate) struct PendingPatch {
    old_schema: CollectionVersion,
    new_schema: CollectionVersion,
    /// The patch writes `IsActive`.
    is_active_explicitly_set: bool,
}

impl<S: Store> crate::database::DB<S> {
    /// Apply a JSON Patch to one or more collection schemas.
    ///
    /// The patch works on a map of every collection version keyed by version
    /// ID; each operation's first path segment names the collection it
    /// changes, by name or version ID. The patch has its GQL values, collection
    /// names and field names replaced with stored values.
    ///
    /// # Arguments
    ///
    /// * `context` - The core params shared by most Defra functions
    /// * `patch` - JSON array of patch operations
    /// * `migration` - Optional Lens migration to apply to documents transiting between the
    ///    old and new versions; the patch must then change exactly one collection
    ///
    /// # Errors
    ///
    /// - `InvalidPatch` if the patch is invalid or cannot be applied, and no
    ///   collection is changed
    /// - `Schema` if a resulting schema is invalid
    #[instrument(skip(self, context, patch, migration), name = "db.patch_collection", fields(collection = tracing::field::Empty))]
    pub async fn patch_collection(
        &self,
        context: &defra_core::RequestContext,
        patch: &str,
        migration: Option<lens::LensConfig>,
    ) -> Result<()> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;

        let raw = validate::operations(patch)?;
        // Every read and write of the patch runs in this one transaction, so
        // the patch commits whole or not at all.
        let mut txn = self.new_txn(false).await?;
        let old_versions = self.get_all_collection_versions_with_txn(&mut txn).await?;
        let versions: RapidHashMap<String, CollectionVersion> = old_versions
            .iter()
            .map(|version| (version.version_id.clone(), version.clone()))
            .collect();
        // Each operation is substituted, checked and applied against the
        // versions the operations before it left, so it can name a field or
        // collection an earlier operation added.
        let mut patched = versions.clone();
        let mut ops = Vec::with_capacity(raw.len());
        for raw_op in &raw {
            // Collections resolve by their stored names, which an earlier
            // operation may have moved away; fields by the current state.
            let op =
                substitute_slop::substitute(&with_leading_slashes(raw_op), &versions, &patched)?;
            let (raw_op, op) = (std::slice::from_ref(raw_op), std::slice::from_ref(&op));
            validate::check_operation(&raw_op[0], &op[0], &patched)?;
            patched = patch_slop::apply_patch(&Value::Array(op.to_vec()), &patched)
                .map_err(|e| without_substituted_ids(e, raw_op, op))?;
            ops.push(op[0].clone());
        }

        let explicitly_active = explicitly_activated(&ops);
        let mut new_versions = Vec::with_capacity(patched.len());
        let mut changed = Vec::new();
        let mut added = Vec::new();
        for mut new_schema in patched.into_values() {
            let old_schema = versions.get(&new_schema.version_id);
            // root_id is runtime storage metadata, absent from the patched JSON.
            if let Some(old_schema) = old_schema {
                new_schema.root_id = old_schema.root_id;
            }
            match old_schema {
                Some(old_schema) if new_schema != *old_schema => {
                    let old_schema = old_schema.clone();
                    prepare_changed(&mut new_schema, &old_schema)?;
                    new_versions.push(new_schema.clone());
                    changed.push(PendingPatch {
                        is_active_explicitly_set: explicitly_active
                            .contains(old_schema.version_id.as_str()),
                        old_schema,
                        new_schema,
                    });
                }
                Some(_) => new_versions.push(new_schema),
                None => {
                    added.push(new_schema.clone());
                    new_versions.push(new_schema);
                }
            }
        }
        let present: RapidHashSet<&str> = new_versions
            .iter()
            .map(|version| version.version_id.as_str())
            .collect();
        let removed: Vec<&CollectionVersion> = old_versions
            .iter()
            .filter(|version| !present.contains(version.version_id.as_str()))
            .collect();

        let names: Vec<&str> = changed
            .iter()
            .map(|patched| patched.old_schema.name.as_str())
            .chain(removed.iter().map(|version| version.name.as_str()))
            .collect();
        tracing::Span::current().record("collection", names.join(",").as_str());

        schema::definition_validation::validate_collection_changes(&old_versions, &new_versions)
            .map_err(Error::InvalidPatch)?;
        for version in &added {
            validate::check_added_ids(&txn, version).await?;
            Collection::validate_default_values(version)?;
            validate_index_fields(version)?;
        }
        for patched in &changed {
            self.validate_changed(&txn, patched).await?;
        }
        if migration.is_some() && changed.len() > 1 {
            return Err(Error::InvalidPatch(
                "a migration applies to one collection, but this patch changes several".to_string(),
            ));
        }

        // Removed versions go first and together: the batch may remove a
        // version and the newer versions built on it, is checked as a whole
        // for relations into the collections it removes, and frees the name
        // for a version the patch activates.
        let version_ids = removed
            .iter()
            .map(|version| version.version_id.clone())
            .collect();
        let deleted = self
            .delete_collection_versions_batch_with_txn(&mut txn, version_ids)
            .await?;
        // Collections are stored in the order the patch first names them, so
        // generated IDs are deterministic; a deactivated version releases its
        // collection's name before another version of it claims the name.
        let order = first_named(&ops);
        changed.sort_by_key(|patched| {
            let version_id = patched.old_schema.version_id.as_str();
            let position = order.iter().position(|named| *named == version_id);
            (!is_deactivated(patched), position)
        });
        let mut migration = migration;
        let mut stored = Vec::new();
        for patched in changed {
            if self.update_version_in_place(&mut txn, &patched).await? {
                continue;
            }
            stored.push(
                self.store_new_version(
                    &mut txn,
                    patched.new_schema,
                    &patched.old_schema,
                    patched.is_active_explicitly_set,
                    migration.take(),
                )
                .await?,
            );
        }
        // A new version of an existing collection whose IDs name stored
        // blocks is stored as given, as Go does.
        for version in &added {
            self.store_added_version(&mut txn, version).await?;
        }
        txn.commit().await?;

        for deleted in &deleted {
            self.after_version_deleted(deleted)?;
        }
        for stored in stored {
            self.after_version_stored(stored).await;
        }
        Ok(())
    }

    /// The checks on a changed version that need the database, read in `txn`.
    async fn validate_changed(&self, txn: &DbTxn<S>, patched: &PendingPatch) -> Result<()> {
        let PendingPatch {
            old_schema,
            new_schema,
            ..
        } = patched;
        Collection::validate_default_values(new_schema)?;
        // Matches Go's validateDematerializedViewHasNoData.
        if old_schema.is_materialized
            && !new_schema.is_materialized
            && self.collection_has_data_with_txn(txn, old_schema).await?
        {
            return Err(Error::InvalidPatch(format!(
                "cannot dematerialize a materialized view that has data, first truncate it and then try again. Name: {}, VersionID: {}",
                old_schema.name, old_schema.version_id
            )));
        }
        validate_index_fields(new_schema)?;
        Ok(())
    }
}

/// Reject indexes that name fields absent from their own collection version.
fn validate_index_fields(version: &CollectionVersion) -> Result<()> {
    for index in &version.indexes {
        for field in &index.fields {
            if !version
                .fields
                .iter()
                .any(|schema_field| schema_field.name == field.name)
            {
                return Err(Error::InvalidPatch(format!(
                    "index '{}' references field '{}' which does not exist in schema",
                    index.name, field.name
                )));
            }
        }
    }
    for index in &version.fulltext_indexes {
        if !version
            .fields
            .iter()
            .any(|schema_field| schema_field.name == index.field_name)
        {
            return Err(Error::InvalidPatch(format!(
                "full-text index on field '{}' references a field which does not exist in schema",
                index.field_name
            )));
        }
    }
    for embedding in &version.vector_embeddings {
        let mut missing = Vec::new();
        if !version
            .fields
            .iter()
            .any(|schema_field| schema_field.name == embedding.field_name)
        {
            missing.push(embedding.field_name.clone());
        }
        for field_name in &embedding.fields {
            if !version
                .fields
                .iter()
                .any(|schema_field| schema_field.name == *field_name)
            {
                missing.push(field_name.clone());
            }
        }
        if !missing.is_empty() {
            return Err(Error::InvalidPatch(format!(
                "vector embedding '{}' references fields which do not exist in schema: {}",
                embedding.field_name,
                missing.join(", ")
            )));
        }
    }
    Ok(())
}

/// Complete a changed version the way Go does before validating it: keep its
/// runtime `root_id`, number its new fields, give new scalar fields the
/// default CRDT, store a new relation to its own collection as the `Self`
/// kind, and add the `_<name>ID` field of each new relation.
fn prepare_changed(
    new_schema: &mut CollectionVersion,
    old_schema: &CollectionVersion,
) -> Result<()> {
    new_schema.root_id = old_schema.root_id;
    clear_copied_field_ids(new_schema, old_schema);
    assign_field_ids(new_schema);

    let old_names: RapidHashSet<&str> = old_schema.fields.iter().map(|f| f.name.as_str()).collect();
    let host = new_schema.name.clone();
    for field in &mut new_schema.fields {
        if old_names.contains(field.name.as_str()) {
            continue;
        }
        if !field.kind.is_relation() && field.crdt_type == schema::CType::None {
            field.crdt_type = schema::CType::LwwRegister;
        }
        if let Some(kind) = self_kind(&field.kind, &host) {
            field.kind = kind;
        }
    }

    let mut next_id = max_field_id(new_schema) + 1;
    new_schema
        .add_relation_id_fields(|| {
            let id = next_id.to_string();
            next_id += 1;
            id
        })
        .map_err(|e| Error::InvalidPatch(format!("failed to add relation id fields: {e}")))
}

/// A patch names a self-referencing relation's kind by its own collection's
/// name (`"Users"`, `"[Users]"` on `Users`); a stored version holds it as the
/// `Self` kind, which the relation validators require.
fn self_kind(kind: &schema::FieldKind, host: &str) -> Option<schema::FieldKind> {
    match kind {
        schema::FieldKind::Named { name, is_array } if name == host => {
            Some(schema::FieldKind::SelfRef {
                relative_id: String::new(),
                is_array: *is_array,
            })
        }
        _ => None,
    }
}

/// A copied field is a new field: a field carrying an existing field's ID
/// under another name, while that field is still present, loses the ID.
fn clear_copied_field_ids(new_schema: &mut CollectionVersion, old_schema: &CollectionVersion) {
    let old_names: RapidHashMap<&str, &str> = old_schema
        .fields
        .iter()
        .map(|f| (f.id.as_str(), f.name.as_str()))
        .collect();
    let kept: RapidHashSet<String> = new_schema
        .fields
        .iter()
        .filter(|f| old_names.get(f.id.as_str()) == Some(&f.name.as_str()))
        .map(|f| f.id.clone())
        .collect();
    for field in &mut new_schema.fields {
        let is_copy = kept.contains(&field.id)
            && old_names.get(field.id.as_str()) != Some(&field.name.as_str());
        if is_copy {
            field.id.clear();
        }
    }
}

fn max_field_id(version: &CollectionVersion) -> u64 {
    version
        .fields
        .iter()
        .filter_map(|field| field.id.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

/// Give each field without a FieldID the next unused one.
fn assign_field_ids(version: &mut CollectionVersion) {
    let first_free = max_field_id(version) + 1;
    let unnumbered = version.fields.iter_mut().filter(|f| f.id.is_empty());
    for (id, field) in (first_free..).zip(unnumbered) {
        field.id = id.to_string();
    }
}

/// The version IDs whose `IsActive` the substituted operations write.
fn explicitly_activated(ops: &[Value]) -> RapidHashSet<&str> {
    ops.iter()
        .filter(|op| {
            matches!(
                op.get("op").and_then(Value::as_str),
                Some("add" | "replace")
            )
        })
        .filter_map(|op| op.get("path").and_then(Value::as_str))
        .filter_map(|path| path.strip_prefix('/')?.strip_suffix("/IsActive"))
        .filter(|version_id| !version_id.contains('/'))
        .collect()
}

/// The collections the substituted operations name, in the order they first
/// name them.
fn first_named(ops: &[Value]) -> Vec<&str> {
    let mut named: Vec<&str> = Vec::new();
    let pointers = ops.iter().flat_map(|op| {
        ["path", "from"]
            .into_iter()
            .filter_map(|member| op.get(member).and_then(Value::as_str))
    });
    for pointer in pointers {
        let collection = pointer.split('/').nth(1).unwrap_or_default();
        if !named.contains(&collection) {
            named.push(collection);
        }
    }
    named
}

fn is_deactivated(patched: &PendingPatch) -> bool {
    patched.old_schema.is_active && !patched.new_schema.is_active
}

/// `error` with the version ID that substitution put in place of a
/// collection's name removed from its paths, so they read as paths within
/// that collection.
fn without_substituted_ids(error: Error, raw: &[Value], ops: &[Value]) -> Error {
    let Error::InvalidPatch(mut message) = error else {
        return error;
    };
    let first = |op: &Value| {
        let path = op.get("path").and_then(Value::as_str).unwrap_or_default();
        path.trim_start_matches('/')
            .split('/')
            .next()
            .unwrap_or_default()
            .to_string()
    };
    for (raw, op) in raw.iter().zip(ops) {
        let (given, version_id) = (first(raw), first(op));
        if given != version_id {
            message = message.replace(&format!("/{version_id}"), "");
        }
    }
    Error::InvalidPatch(message)
}

/// Go accepts collection paths without their leading `/`.
fn with_leading_slashes(op: &Value) -> Value {
    let mut op = op.clone();
    if let Some(members) = op.as_object_mut() {
        for member in ["path", "from"] {
            if let Some(Value::String(pointer)) = members.get_mut(member) {
                if !pointer.starts_with('/') {
                    pointer.insert(0, '/');
                }
            }
        }
    }
    op
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::{CType, FieldDescription, FieldKind, FullTextIndexDescription, IndexDescription};

    fn version() -> CollectionVersion {
        CollectionVersion::new(
            "Users",
            "v1",
            "col-users",
            vec![FieldDescription::new(
                "1",
                "name",
                FieldKind::string(),
                CType::LwwRegister,
            )],
        )
    }

    #[test]
    fn a_missing_index_field_is_refused() {
        let mut version = version();
        version.indexes = vec![IndexDescription::new("bad").with_field("missing", false)];
        let error = validate_index_fields(&version).unwrap_err();
        assert!(error
            .to_string()
            .contains("index 'bad' references field 'missing'"));
    }

    #[test]
    fn a_present_index_field_is_accepted() {
        let mut version = version();
        version.indexes = vec![IndexDescription::new("by_name").with_field("name", false)];
        validate_index_fields(&version).unwrap();
    }

    #[test]
    fn a_missing_fulltext_field_is_refused() {
        let mut version = version();
        version.fulltext_indexes = vec![FullTextIndexDescription::new("missing")];
        let error = validate_index_fields(&version).unwrap_err();
        assert!(error
            .to_string()
            .contains("full-text index on field 'missing' references a field"));
    }

    #[test]
    fn an_embedding_rejects_missing_output_and_source_fields() {
        let mut version = version();
        version.vector_embeddings =
            vec![
                schema::VectorEmbeddingDescription::new("missing_output", "model", "provider")
                    .with_fields(vec!["name".into(), "missing_source".into()]),
            ];
        let error = validate_index_fields(&version).unwrap_err().to_string();
        assert!(error.contains("missing_output"), "{error}");
        assert!(error.contains("missing_source"), "{error}");
        version.vector_embeddings[0].field_name = "name".into();
        version.vector_embeddings[0].fields = vec!["name".into()];
        validate_index_fields(&version).unwrap();
    }
}

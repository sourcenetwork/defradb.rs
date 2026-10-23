//! Schema patching operations for DefraDB.
//!
//! This module implements JSON Patch (RFC 6902) operations for modifying
//! collection schemas. It provides Go DefraDB compatible patching behavior
//! including field addition, removal, and schema version management.

mod apply;
pub mod helpers;
pub mod json;
mod store;
mod validate;
mod version;

use crate::collection::Collection;
use crate::definition::patch::json::{
    extract_field_name_from_path, json_pointer_get, json_pointer_remove, json_pointer_replace,
    json_pointer_set, JsonPatchError,
};
use crate::error::{Error, Result};
use schema::{CollectionSource, CollectionVersion};
use storage::corekv::{Key, OnCommit, Store};
use storage::keys::systemstore::{CollectionKey, CollectionNameKey, CollectionVersionKey};
use tracing::instrument;

impl<S: Store> crate::database::DB<S> {
    /// [`Self::patch_collection_as`] for `identity`, or for the identity acting in [`defra_core::RequestContext::ambient`] when there is none.
    pub async fn patch_collection(
        &self,
        collection_name: &str,
        patch: &str,
        identity: Option<&identity::Did>,
    ) -> Result<CollectionVersion> {
        self.patch_collection_as(
            &crate::access::nac::ambient_as(identity),
            collection_name,
            patch,
        )
        .await
    }

    /// Apply a JSON Patch to a collection schema.
    ///
    /// This method takes a JSON Patch document (RFC 6902) and applies it to
    /// the collection's schema, creating a new schema version.
    ///
    /// # Arguments
    ///
    /// * `collection_name` - The name of the collection to patch (can also be version ID)
    /// * `patch` - JSON array of patch operations
    ///
    /// # Patch Operations
    ///
    /// Supported operations:
    /// - `add` - Add a new field or value
    /// - `remove` - Remove a field or value
    /// - `replace` - Replace an existing value
    /// - `test` - Test that a value exists
    /// - `copy` - Copy a value from one location to another
    /// - `move` - Move a value from one location to another
    ///
    /// # Returns
    ///
    /// The updated collection version (with new version_id).
    ///
    /// # Errors
    ///
    /// - `CollectionNotFound` if the collection doesn't exist
    /// - `InvalidPatch` if the patch is invalid or cannot be applied
    /// - `Schema` if the resulting schema is invalid
    pub async fn patch_collection_as(
        &self,
        context: &defra_core::RequestContext,
        collection_name: &str,
        patch: &str,
    ) -> Result<CollectionVersion> {
        self.patch_collection_with_migration_as(context, collection_name, patch, None)
            .await
    }

    /// [`Self::patch_collection_with_migration_as`] for `identity`, or for the identity acting in [`defra_core::RequestContext::ambient`] when there is none.
    pub async fn patch_collection_with_migration(
        &self,
        collection_name: &str,
        patch: &str,
        migration: Option<lens::LensConfig>,
        identity: Option<&identity::Did>,
    ) -> Result<CollectionVersion> {
        self.patch_collection_with_migration_as(
            &crate::access::nac::ambient_as(identity),
            collection_name,
            patch,
            migration,
        )
        .await
    }

    /// Apply a JSON Patch and its migration as one operation.
    #[instrument(skip(self, patch, migration), fields(collection = %collection_name), name = "db.patch_collection")]
    pub async fn patch_collection_with_migration_as(
        &self,
        context: &defra_core::RequestContext,
        collection_name: &str,
        patch: &str,
        migration: Option<lens::LensConfig>,
    ) -> Result<CollectionVersion> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        // Parse the patch early - needed for both collection lookup fallbacks and processing
        let patch_ops: serde_json::Value =
            serde_json::from_str(patch).map_err(|e| Error::InvalidPatch(e.to_string()))?;

        // Get the current schema - try by name first, then by version ID (including KV store),
        // then check for collection-level move/copy targeting a non-existent collection
        let collection = match self.get_collection(collection_name)? {
            Some(c) => c,
            None => {
                // Try looking up by version ID - search both cache and KV store
                match self
                    .get_collection_by_version_id_full(collection_name)
                    .await?
                {
                    Some(c) => c,
                    None => {
                        // Collection not found by name or version ID.
                        // Check if the patch is a collection-level move/copy where the
                        // "path" targets a non-existent collection (e.g., move /Users → /Books)
                        return self
                            .handle_unknown_collection_patch(collection_name, &patch_ops)
                            .await;
                    }
                }
            }
        };

        let old_schema = collection.schema().clone();
        let actual_name = old_schema.name.clone();
        let old_version_id = old_schema.version_id.clone();
        let collection_id = old_schema.collection_id.clone();
        // Collect known collection names for Kind validation
        let known_collection_names: Vec<String> = self
            .list_collections()
            .unwrap_or_default()
            .into_iter()
            .collect();

        // Apply the patch to the schema JSON
        let mut schema_json = serde_json::to_value(&old_schema)
            .map_err(|e| Error::collection_schema_json("failed to serialize schema to JSON", e))?;

        // Normalize JSON to match Go's serialization format before applying patches.
        // Go always serializes struct fields (null for nil pointers, [] for nil slices),
        // but Rust's skip_serializing_if omits them. Patches targeting these paths
        // need the keys to exist for replace/remove operations to work correctly,
        // and for validators to run instead of json_pointer errors.
        // Note: EncryptedIndexes is NOT pre-populated because Go doesn't expose
        // it in the JSON representation - patches targeting it should fail.
        if let serde_json::Value::Object(ref mut map) = schema_json {
            for key in &["Indexes", "VectorEmbeddings"] {
                map.entry(key.to_string())
                    .or_insert(serde_json::Value::Array(vec![]));
            }
            for key in &["CollectionSet", "Query", "PreviousVersion", "Policy"] {
                map.entry(key.to_string())
                    .or_insert(serde_json::Value::Null);
            }
        }

        // Apply JSON patch operations
        // Go DefraDB embeds collection name in patch paths: /CollectionName/Fields/-
        // We need to strip the collection name prefix to get paths relative to schema.
        // Patches may use the collection name, actual name, or version ID as prefix.
        // Build a list of all recognized prefixes to try stripping.
        let mut strip_prefixes: Vec<String> = vec![format!("/{}/", collection_name)];
        if actual_name != collection_name {
            strip_prefixes.push(format!("/{}/", actual_name));
        }
        if old_version_id != collection_name && old_version_id != actual_name {
            strip_prefixes.push(format!("/{}/", old_version_id));
        }

        let (is_deactivation, is_active_explicitly_set) = self
            .apply_patch_operations(
                patch_ops,
                &mut schema_json,
                &strip_prefixes,
                &known_collection_names,
                &old_schema,
                &collection_id,
            )
            .await?;

        let mut new_schema =
            self.validate_patched_schema(schema_json, &old_schema, &actual_name)?;
        // root_id is runtime storage metadata and is intentionally skipped in schema JSON.
        // Preserve it across patch application so the new active version keeps using the
        // same index/head/cache namespace as the rest of the collection history.
        new_schema.root_id = old_schema.root_id;

        // Matches Go's validateDematerializedViewHasNoData.
        if old_schema.is_materialized
            && !new_schema.is_materialized
            && self.collection_has_data(&old_schema).await?
        {
            return Err(Error::InvalidPatch(format!(
                "cannot dematerialize a materialized view that has data, first truncate it and then try again. Name: {}, VersionID: {}",
                old_schema.name, old_schema.version_id
            )));
        }

        // Handle in-place updates (deactivation, IsActive-only, or PreviousVersion/Transform-only).
        // These don't create a new schema version - they update the existing one.
        let is_isactive_only_change = is_active_explicitly_set
            && new_schema.fields == old_schema.fields
            && new_schema.name == old_schema.name;

        // Check if only PreviousVersion/Transform changed (lens migration linking).
        // This is an in-place update that adds a migration transform to an existing version.
        let is_transform_only_change = !is_deactivation
            && !is_active_explicitly_set
            && new_schema.fields == old_schema.fields
            && new_schema.name == old_schema.name
            && new_schema.is_active == old_schema.is_active
            && new_schema.previous_version != old_schema.previous_version;

        // Check if only metadata changed (VectorEmbeddings, Indexes, IsMaterialized, etc.)
        // without field or name changes. Go treats these as in-place updates.
        let is_metadata_only_change = !is_deactivation
            && !is_active_explicitly_set
            && !is_transform_only_change
            && new_schema.fields == old_schema.fields
            && new_schema.name == old_schema.name
            && new_schema.is_active == old_schema.is_active
            && new_schema.query == old_schema.query
            && new_schema.previous_version == old_schema.previous_version;

        if is_deactivation
            || is_isactive_only_change
            || is_transform_only_change
            || is_metadata_only_change
        {
            // True deletion: remove from store entirely (matches Go behavior)
            if is_deactivation {
                self.delete_collection_version_as(context, &old_version_id, &[])
                    .await?;
                new_schema.is_active = false;
                new_schema.version_id = old_version_id.clone();
                if !is_transform_only_change {
                    new_schema.previous_version = old_schema.previous_version.clone();
                }
                return Ok(new_schema);
            }

            // Keep original version_id
            new_schema.version_id = old_version_id.clone();
            // For IsActive-only, metadata-only, restore original previous_version.
            // For Transform-only changes, keep the new previous_version (contains the transform).
            if !is_transform_only_change {
                new_schema.previous_version = old_schema.previous_version.clone();
            }

            // Validate: can't remove a collection that has documents (only on active→inactive)
            if !new_schema.is_active && old_schema.is_active {
                let has_data = self.collection_has_data(&old_schema).await?;
                if has_data {
                    return Err(Error::InvalidPatch(
                        "cannot delete a collection that has documents, first delete the documents and then delete the version".to_string(),
                    ));
                }
            }

            // Run cross-collection validators to catch issues like multiple active versions
            // against the same snapshot that will store the in-place update.
            let mut txn = self.new_txn(false).await?;
            let all_existing = self.get_all_collection_versions_with_txn(&mut txn).await?;
            let Some(persisted_old_schema) = all_existing.iter().find(|version| {
                version.collection_id == collection_id && version.version_id == old_version_id
            }) else {
                return Err(Error::CollectionVersionNotFound(old_version_id.clone()));
            };
            if !crate::collection::loader::same_schema(persisted_old_schema, &old_schema) {
                return Err(Error::Storage(storage::Error::TxnConflict));
            }
            let name_key = CollectionNameKey::new(&actual_name);
            let persisted_name_version = txn
                .systemstore()?
                .get(&name_key.bytes())
                .await
                .map_err(Error::Storage)?;
            if old_schema.is_active
                && persisted_name_version.as_deref() != Some(old_version_id.as_bytes())
            {
                return Err(Error::Storage(storage::Error::TxnConflict));
            }
            let new_collections: Vec<CollectionVersion> = all_existing
                .iter()
                .filter(|c| c.version_id != old_version_id)
                .cloned()
                .chain(std::iter::once(new_schema.clone()))
                .collect();
            schema::definition_validation::validate_collection_changes(
                &all_existing,
                &new_collections,
            )
            .map_err(Error::InvalidPatch)?;

            // Store the updated version
            {
                let systemstore = txn.systemstore()?;
                let key = CollectionKey::new(&old_version_id);
                let data = serde_json::to_vec(&new_schema).map_err(|e| {
                    Error::collection_schema_json(
                        format!(
                            "failed to serialize updated schema version '{}'",
                            old_version_id
                        ),
                        e,
                    )
                })?;
                systemstore
                    .set(&key.bytes(), &data)
                    .await
                    .map_err(Error::Storage)?;

                // Update name pointer based on activation state
                let name_key = CollectionNameKey::new(&actual_name);
                if new_schema.is_active {
                    systemstore
                        .set(&name_key.bytes(), old_version_id.as_bytes())
                        .await
                        .map_err(Error::Storage)?;
                } else {
                    systemstore
                        .delete(&name_key.bytes())
                        .await
                        .map_err(Error::Storage)?;
                }
            }
            let cached_collection =
                Collection::load_index_actions(new_schema.clone(), &txn.systemstore()?).await?;
            let collections = std::sync::Arc::clone(&self.collections);
            let registered_name = actual_name.clone();
            let active = new_schema.is_active;
            txn.on_commit(OnCommit::OrderedAndWait(Box::new(move || {
                collections.rcu(|old| {
                    let mut cache = old.clone();
                    if active {
                        cache.put_named(&registered_name, cached_collection.clone());
                    } else {
                        cache.remove(&registered_name);
                    }
                    cache
                });
            })))?;
            txn.commit().await?;

            tracing::info!(
                collection = %actual_name,
                version = %old_version_id,
                is_active = new_schema.is_active,
                "Updated collection version in place"
            );

            return Ok(new_schema);
        }

        self.store_new_version(
            new_schema,
            &old_schema,
            &old_version_id,
            &actual_name,
            &collection_id,
            collection_name,
            is_active_explicitly_set,
            migration,
        )
        .await
    }
}

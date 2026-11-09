//! Schema patching operations for DefraDB.
//!
//! This module implements JSON Patch (RFC 6902) operations for modifying
//! collection schemas. It provides Go DefraDB compatible patching behavior
//! including field addition, removal, and schema version management.

mod apply;
pub mod helpers;
mod in_place;
pub mod json;
mod prepare;
mod stage;
mod store;
mod targets;
mod validate;
mod version;

use crate::collection::Collection;
use crate::definition::patch::json::{
    extract_field_name_from_path, json_pointer_get, json_pointer_remove, json_pointer_replace,
    json_pointer_set, JsonPatchError,
};
use crate::error::{Error, Result};
use crate::txn::DbTxn;
use schema::{CollectionSource, CollectionVersion};
use storage::corekv::{Key, OnCommit, Store};
use storage::keys::systemstore::{CollectionKey, CollectionNameKey, CollectionVersionKey};
use tracing::instrument;

/// One collection's share of a patch, applied and validated on its own but
/// not yet staged.
pub(crate) struct PreparedPatch {
    /// The name or version ID the patch addressed the collection by.
    pub collection_name: String,
    /// The version the patch started from.
    pub old_schema: CollectionVersion,
    /// The patched version; its version ID is still the old one.
    pub new_schema: CollectionVersion,
    /// The patch removes the whole version.
    pub is_deactivation: bool,
    /// The patch sets `IsActive` itself.
    pub is_active_explicitly_set: bool,
}

/// A staged patch step, and what it owes once its transaction commits.
pub(crate) struct PatchOutcome {
    pub schema: CollectionVersion,
    pub committed_migration: Option<(lens::TransformId, lens::LensConfig)>,
    pub is_new_version: bool,
}

/// `existing` as the patch will leave it: each of `siblings` (patched, not
/// yet staged) in place of the stored version it shares a version ID with.
pub(crate) fn with_siblings(
    existing: &[CollectionVersion],
    siblings: &[CollectionVersion],
) -> Vec<CollectionVersion> {
    existing
        .iter()
        .map(|version| {
            siblings
                .iter()
                .find(|sibling| sibling.version_id == version.version_id)
                .unwrap_or(version)
                .clone()
        })
        .collect()
}

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

    /// Apply a JSON Patch and its migration as one operation, every path read
    /// as naming `collection_name`.
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
        let ops = parse_patch(patch)?;
        let ops = ops
            .as_array()
            .cloned()
            .ok_or_else(|| Error::InvalidPatch("patch must be a JSON array".into()))?;
        let mut txn = self.new_txn(false).await?;
        let staged = self
            .stage_groups(&mut txn, vec![(collection_name.to_owned(), ops)], migration)
            .await?;
        txn.commit().await?;
        let mut patched = self.finish_staged(staged).await?;
        patched
            .pop()
            .ok_or_else(|| Error::CollectionNotFound(collection_name.to_owned()))
    }

    /// Stage `prepared` on `txn`: in place when it keeps the version, as a new
    /// version otherwise.
    pub(crate) async fn write_prepared(
        &self,
        txn: &mut DbTxn<S>,
        prepared: &PreparedPatch,
        siblings: &[CollectionVersion],
        migration: Option<lens::LensConfig>,
    ) -> Result<PatchOutcome> {
        if prepared.is_in_place() {
            self.write_in_place(txn, prepared, siblings).await
        } else {
            self.store_new_version(txn, prepared, siblings, migration)
                .await
        }
    }
}

impl<S> crate::database::DB<S>
where
    S: Store,
{
    /// Go's `PatchCollection`: apply `patch` to the collections its paths
    /// name (`/<Name>/...` or `/<VersionID>/...`) for `context`'s caller,
    /// with `migration` as the migration to each version it adds, in one
    /// transaction of its own.
    pub async fn patch_collections_as(
        &self,
        context: &defra_core::RequestContext,
        patch: &str,
        migration: Option<lens::LensConfig>,
    ) -> Result<()> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        let ops = parse_patch(patch)?;
        let mut txn = self.new_txn(false).await?;
        let staged = self.stage_patch(&mut txn, ops, migration).await?;
        txn.commit().await?;
        self.finish_staged(staged).await.map(|_| ())
    }
}

/// A patch document: a JSON array of operations.
pub(crate) fn parse_patch(patch: &str) -> Result<serde_json::Value> {
    serde_json::from_str(patch).map_err(|e| Error::InvalidPatch(e.to_string()))
}

/// The version a patch path's first segment names among `versions`: the
/// active one of that name, else the one with that version ID.
pub(crate) fn find_target<'v>(
    versions: &'v [CollectionVersion],
    name: &str,
) -> Option<&'v CollectionVersion> {
    versions
        .iter()
        .find(|version| version.is_active && version.name == name)
        .or_else(|| versions.iter().find(|version| version.version_id == name))
}

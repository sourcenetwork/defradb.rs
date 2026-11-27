//! Collection definitions and lenses as the caller `context` names, in the
//! transaction it carries if it carries one.

use defra_core::RequestContext;
use query::documents::Filter;
use schema::{CollectionSelector, CollectionVersion};
use serde_json::{Map, Value};

use super::NodeCore;
use crate::ops::transaction_error;
use crate::{Error, Result};

impl NodeCore<'_> {
    /// Add the collections an SDL document defines. Outside a transaction a
    /// call that names no identity acts as the node.
    pub(crate) async fn add_collection(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> Result<Vec<CollectionVersion>> {
        let context = self.node.admitted_caller(context)?;
        if context.txn().is_some() {
            return self
                .node
                .runner()
                .add_collection_as(&context, sdl)
                .await
                .map_err(transaction_error);
        }

        let (ops, sdl, context) = (
            self.node.schema_ops.clone(),
            sdl.to_owned(),
            self.node.txn_owner(&context),
        );
        self.node
            .on_pool(move || async move { ops.add_collection(&context, &sdl).await })
            .await
    }

    /// Apply the JSON patch `patch` to the collection definitions, with
    /// `migration`, a lens (Go's `model.Lens`), as the migration to the
    /// versions it adds. Go's `PatchCollection`.
    pub(crate) async fn patch_collection(
        &self,
        context: &RequestContext,
        patch: &str,
        migration: Option<&Value>,
    ) -> Result<()> {
        let migration = migration.map(lens_config).transpose()?;
        self.node
            .runner()
            .patch_collection_as(&self.node.admitted_caller(context)?, patch, migration)
            .await
            .map_err(transaction_error)
    }

    /// Make collection version `version_id` its collection's active one.
    /// Go's `SetActiveCollectionVersion`.
    pub(crate) async fn set_active_collection_version(
        &self,
        context: &RequestContext,
        version_id: &str,
    ) -> Result<()> {
        self.node
            .runner()
            .set_active_collection_version_as(&self.node.admitted_caller(context)?, version_id)
            .await
            .map_err(transaction_error)
    }

    /// Remove the documents of collection `name` that `filter` matches, or
    /// all of them without one, keeping the collection. Go's
    /// `Collection.Truncate`.
    pub(crate) async fn truncate_collection(
        &self,
        context: &RequestContext,
        name: &str,
        filter: Option<&Filter>,
    ) -> Result<()> {
        self.node
            .runner()
            .truncate_collection_as(&self.node.admitted_caller(context)?, name, filter)
            .await
            .map_err(transaction_error)
    }

    /// Set the lens migration `config` (Go's `LensConfig`) between two
    /// collection versions and return its lens ID. Go's `SetMigration`.
    pub(crate) async fn set_migration(
        &self,
        context: &RequestContext,
        config: &Value,
    ) -> Result<String> {
        self.node
            .runner()
            .set_migration_as(&self.node.admitted_caller(context)?, &config.to_string())
            .await
            .map_err(transaction_error)
    }

    /// Store `lens` (Go's `model.Lens`) and return its content ID. Go's
    /// `AddLens`.
    pub(crate) async fn add_lens(&self, context: &RequestContext, lens: &Value) -> Result<String> {
        self.node
            .runner()
            .add_lens_as(&self.node.admitted_caller(context)?, lens_config(lens)?)
            .await
            .map_err(transaction_error)
    }

    /// Every stored lens, by content ID. Go's `ListLenses`.
    pub(crate) async fn list_lenses(&self, context: &RequestContext) -> Result<Map<String, Value>> {
        let listed = self
            .node
            .runner()
            .list_lenses_as(&self.node.admitted_caller(context)?)
            .await
            .map_err(transaction_error)?;
        serde_json::from_str(&listed).map_err(Error::LensJson)
    }

    /// The collection versions `selector` picks, ordered by name then
    /// version ID. Go's `GetCollections`.
    pub(crate) async fn get_collections(
        &self,
        context: &RequestContext,
        selector: &CollectionSelector,
    ) -> Result<Vec<CollectionVersion>> {
        self.node
            .runner()
            .get_collections_as(&self.node.admitted_caller(context)?, selector)
            .await
            .map_err(transaction_error)
    }

    /// Eagerly migrate and cache every known-version document in a
    /// collection; the number of documents advanced to the active version.
    /// A datastore-only operation: it creates and broadcasts no commits, and
    /// runs in no explicit transaction.
    pub(crate) async fn materialize_collection(
        &self,
        context: &RequestContext,
        collection_name: &str,
    ) -> Result<usize> {
        self.node.admit(context)?;
        let ops = self.node.schema_ops.clone();
        let context = context.clone();
        let name = collection_name.to_owned();
        self.node
            .on_pool(move || async move { ops.materialize_collection(&context, &name).await })
            .await
    }
}

/// A lens or lens migration in Go's JSON shape.
fn lens_config(lens: &Value) -> Result<lens::LensConfig> {
    serde_json::from_value(lens.clone()).map_err(Error::LensJson)
}

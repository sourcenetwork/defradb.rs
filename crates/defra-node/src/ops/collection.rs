//! Collection definitions and lenses as the caller `context` names, in the
//! transaction it carries if it carries one.

use defra_core::RequestContext;
use schema::{CollectionSelector, CollectionVersion};
use serde_json::{Map, Value};

use crate::ops::request::transaction_error;
use crate::{Error, Node, Result};

impl Node {
    /// Apply the JSON patch `patch` to the collection definitions, with
    /// `migration`, a lens (Go's `model.Lens`), as the migration to the
    /// versions it adds. Go's `PatchCollection`.
    pub async fn patch_collection_as(
        &self,
        context: &RequestContext,
        patch: &str,
        migration: Option<&Value>,
    ) -> Result<()> {
        let migration = migration.map(lens_config).transpose()?;
        self.runner()
            .patch_collection_as(&self.caller(context), patch, migration)
            .await
            .map_err(transaction_error)
    }

    /// Make collection version `version_id` its collection's active one.
    /// Go's `SetActiveCollectionVersion`.
    pub async fn set_active_collection_version_as(
        &self,
        context: &RequestContext,
        version_id: &str,
    ) -> Result<()> {
        self.runner()
            .set_active_collection_version_as(&self.caller(context), version_id)
            .await
            .map_err(transaction_error)
    }

    /// Set the lens migration `config` (Go's `LensConfig`) between two
    /// collection versions and return its lens ID. Go's `SetMigration`.
    pub async fn set_migration_as(
        &self,
        context: &RequestContext,
        config: &Value,
    ) -> Result<String> {
        self.runner()
            .set_migration_as(&self.caller(context), &config.to_string())
            .await
            .map_err(transaction_error)
    }

    /// Store `lens` (Go's `model.Lens`) and return its content ID. Go's
    /// `AddLens`.
    pub async fn add_lens_as(&self, context: &RequestContext, lens: &Value) -> Result<String> {
        self.runner()
            .add_lens_as(&self.caller(context), lens_config(lens)?)
            .await
            .map_err(transaction_error)
    }

    /// Every stored lens, by content ID. Go's `ListLenses`.
    pub async fn list_lenses_as(&self, context: &RequestContext) -> Result<Map<String, Value>> {
        let listed = self
            .runner()
            .list_lenses_as(&self.caller(context))
            .await
            .map_err(transaction_error)?;
        serde_json::from_str(&listed).map_err(Error::LensJson)
    }

    /// The collection versions `selector` picks, ordered by name then
    /// version ID. Go's `GetCollections`.
    pub async fn get_collections_as(
        &self,
        context: &RequestContext,
        selector: &CollectionSelector,
    ) -> Result<Vec<CollectionVersion>> {
        self.runner()
            .get_collections_as(&self.caller(context), selector)
            .await
            .map_err(transaction_error)
    }
}

/// A lens or lens migration in Go's JSON shape.
fn lens_config(lens: &Value) -> Result<lens::LensConfig> {
    serde_json::from_value(lens.clone()).map_err(Error::LensJson)
}

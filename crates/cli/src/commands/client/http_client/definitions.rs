//! Collection definitions and lenses over Go's HTTP API.

use reqwest::{Method, Response};
use schema::{CollectionSelector, CollectionVersion};
use serde_json::{json, Map, Value};

use super::HttpClient;
use crate::error::{Error, Result};

impl HttpClient {
    /// Apply the JSON patch `patch` to the collection definitions, with
    /// `migration`, a lens (Go's `model.Lens`), as the migration to the
    /// versions it adds. Go's `PatchCollection`.
    pub async fn patch_collection(
        &self,
        txn: Option<&str>,
        patch: &str,
        migration: Option<&Value>,
    ) -> Result<()> {
        let mut body = json!({ "Patch": patch });
        if let Some(migration) = migration {
            body["Migration"] = migration.clone();
        }
        self.send(
            Method::PATCH,
            "collections",
            &[],
            Some(body.to_string()),
            txn,
        )
        .await
        .map(drop)
    }

    /// Make collection version `version_id` its collection's active one.
    /// Go's `SetActiveCollectionVersion`.
    pub async fn set_active_collection_version(
        &self,
        txn: Option<&str>,
        version_id: &str,
    ) -> Result<()> {
        let body = Some(version_id.to_owned());
        self.send(Method::POST, "collections/default", &[], body, txn)
            .await
            .map(drop)
    }

    /// Set the lens migration `config` (Go's `LensConfig`) between two
    /// collection versions and return its lens ID. Go's `SetMigration`.
    pub async fn set_migration(&self, txn: Option<&str>, config: &Value) -> Result<String> {
        let body = Some(config.to_string());
        let response = self
            .send(Method::POST, "collections/migrations", &[], body, txn)
            .await?;
        lens_id(response).await
    }

    /// Store `lens` (Go's `model.Lens`) and return its content ID. Go's
    /// `AddLens`.
    pub async fn add_lens(&self, txn: Option<&str>, lens: &Value) -> Result<String> {
        let body = Some(json!({ "lens": lens }).to_string());
        let response = self.send(Method::POST, "lens", &[], body, txn).await?;
        lens_id(response).await
    }

    /// Every stored lens, by content ID. Go's `ListLenses`.
    pub async fn list_lenses(&self, txn: Option<&str>) -> Result<Map<String, Value>> {
        let response = self.send(Method::GET, "lens", &[], None, txn).await?;
        let mut body: Value = response.json().await?;
        match body.get_mut("lenses").map(Value::take) {
            Some(Value::Object(lenses)) => Ok(lenses),
            Some(Value::Null) | None => Ok(Map::new()),
            Some(other) => Err(Error::Server(format!("unexpected lenses: {other}"))),
        }
    }

    /// The collection versions `selector` picks, ordered by name then
    /// version ID. Go's `GetCollections`.
    pub async fn get_collections(
        &self,
        txn: Option<&str>,
        selector: &CollectionSelector,
    ) -> Result<Vec<CollectionVersion>> {
        let query = selector_query(selector);
        let response = self
            .send(Method::GET, "collections", &query, None, txn)
            .await?;
        Ok(response.json().await?)
    }
}

/// The `lensId` of Go's set-migration and add-lens answers.
async fn lens_id(response: Response) -> Result<String> {
    let body: Value = response.json().await?;
    body.get("lensId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::Server(format!("no lensId in {body}")))
}

/// Go's `GetCollections` query parameters.
fn selector_query(selector: &CollectionSelector) -> Vec<(&'static str, String)> {
    let mut query = Vec::new();
    if let Some(names) = &selector.names {
        query.extend(names.iter().map(|name| ("name", name.clone())));
    }
    if let Some(version_id) = &selector.version_id {
        query.push(("version_id", version_id.clone()));
    }
    if let Some(collection_id) = &selector.collection_id {
        query.push(("collection_id", collection_id.clone()));
    }
    if selector.get_inactive {
        query.push(("get_inactive", "true".to_owned()));
    }
    query
}

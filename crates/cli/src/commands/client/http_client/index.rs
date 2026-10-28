//! Index HTTP client methods

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use urlencoding::encode;

use super::HttpClient;
use crate::error::Result;

/// Go's index create body (`client.IndexDescriptionCreateRequest`).
#[derive(Debug, Serialize)]
struct GoIndexCreateRequest {
    #[serde(rename = "Name", skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(rename = "Fields")]
    fields: Vec<GoIndexedField>,
    #[serde(rename = "Unique", skip_serializing_if = "std::ops::Not::not")]
    unique: bool,
    #[serde(rename = "Vector", skip_serializing_if = "Option::is_none")]
    vector: Option<schema::VectorIndexDescription>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GoIndexedField {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Descending", default)]
    descending: bool,
}

/// Go's index description as `POST /collections/{name}/indexes` returns it.
#[derive(Debug, Deserialize)]
struct GoIndexDescription {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Fields")]
    fields: Vec<GoIndexedField>,
    #[serde(rename = "Unique", default)]
    unique: bool,
    #[serde(flatten)]
    kind: schema::IndexKind,
}

#[derive(Debug, Deserialize)]
struct GoListIndexesResult {
    #[serde(rename = "Description")]
    description: GoIndexDescription,
}

impl GoIndexDescription {
    fn into_info(self, collection: &str) -> IndexInfo {
        let kind = match self.kind {
            schema::IndexKind::Ordered(_) => None,
            kind => Some(kind),
        };
        IndexInfo {
            name: self.name,
            collection: collection.to_string(),
            fields: self
                .fields
                .into_iter()
                .map(|field| IndexFieldInfo {
                    name: field.name,
                    direction: Some(if field.descending { "DESC" } else { "ASC" }.to_string()),
                })
                .collect(),
            unique: self.unique,
            kind,
        }
    }
}

/// Index info from list/create
#[derive(Debug, Deserialize, Serialize)]
pub struct IndexInfo {
    /// Index name
    #[serde(rename = "name", alias = "Name")]
    pub name: String,

    /// Collection name
    #[serde(rename = "collection", alias = "Collection")]
    pub collection: String,

    /// Fields in the index
    #[serde(rename = "fields", alias = "Fields")]
    pub fields: Vec<IndexFieldInfo>,

    /// Whether the index is unique
    #[serde(rename = "unique", alias = "Unique", default)]
    pub unique: bool,

    /// Kind-specific configuration, absent for an ordinary index.
    ///
    /// Without this the client parsed the server's response into a struct that
    /// had nowhere to put it, so `index list` printed a vector index as an
    /// ordinary one: the same silent downgrade the wire envelope exists to
    /// prevent, one layer above the wire.
    #[serde(
        rename = "kind",
        alias = "Kind",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub kind: Option<schema::IndexKind>,
}

/// Index field info
#[derive(Debug, Deserialize, Serialize)]
pub struct IndexFieldInfo {
    /// Field name
    #[serde(rename = "name", alias = "Name")]
    pub name: String,

    /// Sort direction (ASC or DESC)
    #[serde(rename = "direction", alias = "Direction", default)]
    pub direction: Option<String>,
}

impl HttpClient {
    pub async fn index_create(
        &self,
        collection: &str,
        fields: &[String],
        name: Option<&str>,
        unique: bool,
        vector: Option<schema::VectorIndexDescription>,
    ) -> Result<IndexInfo> {
        let url = format!(
            "{}/api/v0/collections/{}/indexes",
            self.base_url,
            encode(collection)
        );
        let request = GoIndexCreateRequest {
            name: name.map(|s| s.to_string()),
            fields: fields
                .iter()
                .map(|name| GoIndexedField {
                    name: name.clone(),
                    descending: false,
                })
                .collect(),
            unique,
            vector,
        };
        let created: GoIndexDescription = self.post_json(&url, &request).await?;
        Ok(created.into_info(collection))
    }

    pub async fn index_list(&self, collection: Option<&str>) -> Result<Vec<IndexInfo>> {
        match collection {
            Some(col) => {
                let url = format!(
                    "{}/api/v0/collections/{}/indexes",
                    self.base_url,
                    encode(col)
                );
                let results: Vec<GoListIndexesResult> =
                    self.request_json("GET", &url, None).await?;
                Ok(results
                    .into_iter()
                    .map(|result| result.description.into_info(col))
                    .collect())
            }
            None => {
                let url = format!("{}/api/v0/collections/indexes", self.base_url);
                let grouped: BTreeMap<String, Vec<GoListIndexesResult>> =
                    self.request_json("GET", &url, None).await?;
                Ok(grouped
                    .into_iter()
                    .flat_map(|(col, results)| {
                        results
                            .into_iter()
                            .map(move |result| result.description.into_info(&col))
                    })
                    .collect())
            }
        }
    }

    pub async fn index_delete(&self, collection: &str, name: &str) -> Result<()> {
        let url = format!(
            "{}/api/v0/collections/{}/indexes/{}",
            self.base_url,
            encode(collection),
            encode(name)
        );
        self.request_void("DELETE", &url, None).await
    }
}

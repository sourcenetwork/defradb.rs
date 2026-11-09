//! Encrypted index HTTP client methods

use net::http::Method;
use serde::{Deserialize, Serialize};
use urlencoding::encode;

use super::HttpClient;
use crate::error::Result;

/// Encrypted index info from list/add
#[derive(Debug, Deserialize, Serialize)]
pub struct EncryptedIndexInfo {
    #[serde(rename = "FieldName")]
    pub field_name: String,
    #[serde(rename = "Type")]
    pub index_type: String,
}

impl HttpClient {
    /// Add an encrypted index on a collection field, in transaction `txn`
    /// when given.
    pub async fn encrypted_index_add(
        &self,
        txn: Option<&str>,
        collection: &str,
        field: &str,
    ) -> Result<EncryptedIndexInfo> {
        let path = format!("collections/{}/encrypted-indexes", encode(collection));
        let body = serde_json::to_string(&serde_json::json!({ "FieldName": field }))
            .map_err(|e| crate::error::Error::Server(e.to_string()))?;
        Ok(self
            .send(Method::POST, &path, &[], Some(body), txn)
            .await?
            .json()
            .await?)
    }

    /// List encrypted indexes for a collection, in transaction `txn` when
    /// given.
    pub async fn encrypted_index_list(
        &self,
        txn: Option<&str>,
        collection: &str,
    ) -> Result<Vec<EncryptedIndexInfo>> {
        let path = format!("collections/{}/encrypted-indexes", encode(collection));
        Ok(self
            .send(Method::GET, &path, &[], None, txn)
            .await?
            .json()
            .await?)
    }

    /// Every collection's encrypted indexes, by collection name.
    pub async fn encrypted_index_list_all(
        &self,
    ) -> Result<std::collections::BTreeMap<String, Vec<EncryptedIndexInfo>>> {
        let url = format!("{}/api/v0/encrypted-indexes", self.base_url);
        self.request_json("GET", &url, None).await
    }

    /// Delete an encrypted index from a collection field, in transaction
    /// `txn` when given.
    pub async fn encrypted_index_delete(
        &self,
        txn: Option<&str>,
        collection: &str,
        field: &str,
    ) -> Result<()> {
        let path = format!(
            "collections/{}/encrypted-indexes/{}",
            encode(collection),
            encode(field)
        );
        self.send(Method::DELETE, &path, &[], None, txn)
            .await
            .map(drop)
    }
}

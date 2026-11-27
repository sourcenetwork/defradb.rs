//! Collection HTTP client methods

use net::http::Method;
use urlencoding::encode;

use super::HttpClient;
use crate::error::Result;

impl HttpClient {
    /// Remove the documents of collection `name` that `filter` matches, or
    /// all of them, in transaction `txn`. Go's `Collection.Truncate`.
    pub async fn collection_truncate(
        &self,
        txn: Option<&str>,
        name: &str,
        filter: Option<&serde_json::Value>,
    ) -> Result<()> {
        let path = format!("collections/{}/truncate", encode(name));
        let body = filter.map(|filter| serde_json::json!({ "filter": filter }).to_string());
        self.send(Method::DELETE, &path, &[], body, txn)
            .await
            .map(drop)
    }

    /// Hard-delete the documents of collection `name` named by `doc_ids`, in
    /// transaction `txn`; the response, `{"count": purged}`. Go shinzo's
    /// `Collection.PurgeByDocIDs`.
    pub async fn collection_purge_docs(
        &self,
        txn: Option<&str>,
        name: &str,
        doc_ids: &[String],
    ) -> Result<serde_json::Value> {
        let path = format!("collections/{}/documents/purge", encode(name));
        let body = serde_json::json!({ "docIDs": doc_ids }).to_string();
        let response = self
            .send(Method::DELETE, &path, &[], Some(body), txn)
            .await?;
        Ok(response.json().await?)
    }

    /// Delete one or more collections by name via Go-compatible
    /// `DELETE /collections?name=Users,Books&active-only=true`.
    pub async fn collection_delete(&self, names: &[String], active_only: bool) -> Result<()> {
        let joined = names
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let url = format!(
            "{}/api/v0/collections?name={}&active-only={}",
            self.base_url,
            encode(&joined),
            active_only
        );
        self.request_void("DELETE", &url, None).await
    }
}

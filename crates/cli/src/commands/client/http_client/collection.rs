//! Collection HTTP client methods

use reqwest::Method;
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

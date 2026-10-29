//! Collection HTTP client methods

use urlencoding::encode;

use super::HttpClient;
use crate::error::Result;

impl HttpClient {
    pub async fn collection_truncate(
        &self,
        name: &str,
        filter: Option<&serde_json::Value>,
    ) -> Result<()> {
        let url = format!(
            "{}/api/v0/collections/{}/truncate",
            self.base_url,
            encode(name)
        );
        let body = filter
            .map(|filter| serde_json::to_string(&serde_json::json!({ "filter": filter })))
            .transpose()?;
        self.request_void("DELETE", &url, body.as_deref()).await
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

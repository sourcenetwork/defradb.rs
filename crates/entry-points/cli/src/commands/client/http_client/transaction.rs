//! Transaction HTTP client methods

use super::types::TxBeginResponse;
use super::HttpClient;
use crate::error::Result;
use serde_json::Value;

impl HttpClient {
    /// Begin a new transaction.
    ///
    /// POST /api/v0/tx?read_only=true
    pub async fn tx_begin(&self, read_only: bool) -> Result<TxBeginResponse> {
        let mut url = format!("{}/api/v0/tx", self.base_url);
        if read_only {
            url.push_str("?read_only=true");
        }
        self.request_json("POST", &url, None).await
    }

    /// Commit a transaction.
    ///
    /// POST /api/v0/tx/{id}
    pub async fn tx_commit(&self, txn_id: &str) -> Result<()> {
        let url = format!("{}/api/v0/tx/{}", self.base_url, txn_id);
        // Never resent: a commit answered with an unknown outcome may still
        // land, and a second attempt would report it as not found.
        let response = self.send_with_retries("POST", &url, None, 0).await?;
        if !response.status().is_success() {
            return Err(Self::extract_error("POST", response).await);
        }
        Ok(())
    }

    /// Discard/rollback a transaction.
    ///
    /// DELETE /api/v0/tx/{id}
    pub async fn tx_discard(&self, txn_id: &str) -> Result<()> {
        let url = format!("{}/api/v0/tx/{}", self.base_url, txn_id);
        self.request_void("DELETE", &url, None).await
    }

    /// Read a transaction's outcome, with the HTTP status and body.
    pub async fn tx_outcome(&self, txn_id: &str) -> Result<(u16, Value)> {
        let url = format!("{}/api/v0/tx/{}", self.base_url, txn_id);
        let response = self.send_with_retry("GET", &url, None).await?;
        let status = response.status().as_u16();
        let body = response.json().await?;
        Ok((status, body))
    }
}

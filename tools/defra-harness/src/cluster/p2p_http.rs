//! P2P endpoints that only the node's HTTP API serves; the CLI client has no
//! subcommand for them. `sync/status` and `peers/{id}` are Rust-only.

use crate::error::{Result, WrapErr};
use serde_json::Value;

use super::runtime::TestCluster;

async fn checked(response: reqwest::Response, what: &str) -> Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    Err(crate::error::err!("{what}: HTTP {status}: {}", body.trim()))
}

impl TestCluster {
    async fn p2p_get(&self, index: usize, path: &str) -> Result<Value> {
        let url = format!("{}/api/v0/p2p/{path}", self.api_url(index));
        let response = reqwest::get(&url)
            .await
            .wrap_err_with(|| format!("GET {url}"))?;
        checked(response, &url)
            .await?
            .json()
            .await
            .wrap_err_with(|| format!("GET {url}: body is not JSON"))
    }

    /// The node's sync-resource snapshot (`GET /api/v0/p2p/sync/status`):
    /// pending-DAG depth and counters, push backlog, and the durable
    /// push-retry marker counts under `push_retry_markers`.
    pub async fn p2p_sync_status(&self, index: usize) -> Result<Value> {
        self.p2p_get(index, "sync/status").await
    }

    /// What the node knows about one peer (`GET /api/v0/p2p/peers/{peer_id}`),
    /// including `push_retries`: the durable retry markers it still owes that
    /// peer, or null when it owes none.
    pub async fn p2p_peer_details(&self, index: usize, peer_id: &str) -> Result<Value> {
        self.p2p_get(index, &format!("peers/{peer_id}")).await
    }

    /// Close the node's connections to `addresses`
    /// (`POST /api/v0/p2p/disconnect`); the CLI has no disconnect command.
    pub async fn p2p_disconnect(&self, index: usize, addresses: &[&str]) -> Result<()> {
        let url = format!("{}/api/v0/p2p/disconnect", self.api_url(index));
        let response = reqwest::Client::new()
            .post(&url)
            .json(addresses)
            .send()
            .await
            .wrap_err_with(|| format!("POST {url}"))?;
        checked(response, &url).await.map(|_| ())
    }
}

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Bytes, B256};
use defra_core::thread_bounds::MaybeBoxFuture;
use serde::de::DeserializeOwned;
use vera_domain::{ConsensusPublicKey, NativeTx, ReceiptResponse, RECEIPT_RESPONSE_BYTES};

use super::transport::RpcTransport;

#[cfg(not(target_arch = "wasm32"))]
use net::http as http_client;
#[cfg(target_arch = "wasm32")]
use reqwest as http_client;

pub struct VeraRsClient {
    transport: Arc<dyn RpcTransport>,
}

/// JSON-RPC over HTTP to the node at `url`.
struct HttpTransport {
    url: String,
    http: http_client::Client,
    next_id: AtomicU64,
}

impl HttpTransport {
    fn new(url: String, request_timeout: Duration, edge: &edge::Edge) -> Result<Self, ClientError> {
        #[cfg(not(target_arch = "wasm32"))]
        let http = http_client::Client::builder(edge.clone());
        #[cfg(target_arch = "wasm32")]
        let http = {
            let _ = edge;
            http_client::Client::builder()
        };
        Ok(Self {
            url,
            http: http.timeout(request_timeout).build()?,
            next_id: AtomicU64::new(1),
        })
    }

    async fn post(
        &self,
        method: &str,
        params: serde_json::Value,
        maximum: usize,
    ) -> Result<serde_json::Value, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut response = self
            .http
            .post(&self.url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "id": id, "method": method, "params": params,
            }))
            .send()
            .await?
            .error_for_status()?;
        if response
            .content_length()
            .is_some_and(|size| size > maximum as u64)
        {
            return Err(ClientError::InvalidResponse("response exceeds byte limit"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if chunk.len() > maximum - bytes.len() {
                return Err(ClientError::InvalidResponse("response exceeds byte limit"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
        if value["jsonrpc"] != "2.0" || value["id"].as_u64() != Some(id) {
            return Err(ClientError::InvalidResponse(
                "request ID or protocol version mismatch",
            ));
        }
        if let Some(error) = value.get("error") {
            return Err(ClientError::Rpc {
                code: error["code"].as_i64().unwrap_or(0),
                message: error["message"]
                    .as_str()
                    .unwrap_or("unspecified error")
                    .into(),
            });
        }
        Ok(value
            .get_mut("result")
            .ok_or(ClientError::InvalidResponse("missing result"))?
            .take())
    }
}

impl RpcTransport for HttpTransport {
    fn call<'a>(
        &'a self,
        method: &'a str,
        params: serde_json::Value,
        max_bytes: usize,
    ) -> MaybeBoxFuture<'a, Result<serde_json::Value, ClientError>> {
        Box::pin(self.post(method, params, max_bytes))
    }
}

impl VeraRsClient {
    /// A client of the node at `url` whose connections run on `edge`.
    pub fn new(
        url: String,
        request_timeout: Duration,
        edge: &edge::Edge,
    ) -> Result<Self, ClientError> {
        Ok(Self::with_transport(Arc::new(HttpTransport::new(
            url,
            request_timeout,
            edge,
        )?)))
    }

    /// A client that reaches the node through `transport`.
    pub fn with_transport(transport: Arc<dyn RpcTransport>) -> Self {
        Self { transport }
    }

    async fn rpc<T>(
        &self,
        method: &str,
        params: serde_json::Value,
        maximum: usize,
    ) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
    {
        let result = self.transport.call(method, params, maximum).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn send(&self, wire: &[u8]) -> Result<B256, ClientError> {
        let expected = NativeTx::decode_wire(wire)
            .map_err(|_| ClientError::InvalidResponse("invalid signed request"))?
            .tx_id()
            .0;
        let returned: B256 = self
            .rpc(
                "vera_sendNativeTx",
                serde_json::json!([Bytes::copy_from_slice(wire)]),
                4096,
            )
            .await?;
        if returned != expected {
            return Err(ClientError::InvalidResponse("submission ID mismatch"));
        }
        Ok(expected)
    }

    pub async fn receipt(
        &self,
        hash: B256,
        trusted: &ConsensusPublicKey,
    ) -> Result<Option<ReceiptResponse>, ClientError> {
        let response: Option<ReceiptResponse> = self
            .rpc(
                "vera_getReceiptProof",
                serde_json::json!([hash]),
                RECEIPT_RESPONSE_BYTES,
            )
            .await?;
        if let Some(response) = &response {
            response.verify(hash, trusted)?;
        }
        Ok(response)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Http(#[from] http_client::Error),
    #[error("RPC error ({code}): {message}")]
    Rpc { code: i64, message: String },
    #[error("invalid RPC response: {0}")]
    InvalidResponse(&'static str),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Receipt(#[from] vera_domain::ReceiptResponseError),
}

impl ClientError {
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Http(_)
                | Self::Rpc {
                    code: -32002 | -32000,
                    ..
                }
        )
    }
}

#[cfg(test)]
mod tests;

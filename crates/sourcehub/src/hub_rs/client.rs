use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use alloy_primitives::{Address, Bytes, B256};
use hub_domain::{ConsensusPublicKey, NativeTx, ReceiptResponse, RECEIPT_RESPONSE_BYTES};
use serde::de::DeserializeOwned;

pub struct HubRsClient {
    url: String,
    http: reqwest::Client,
    next_id: AtomicU64,
}

impl HubRsClient {
    pub fn new(url: String, request_timeout: Duration) -> Result<Self, ClientError> {
        Ok(Self {
            url,
            http: reqwest::Client::builder()
                .timeout(request_timeout)
                .build()?,
            next_id: AtomicU64::new(1),
        })
    }

    async fn rpc<T: DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
        maximum: usize,
    ) -> Result<T, ClientError> {
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
        let result = value
            .get_mut("result")
            .ok_or(ClientError::InvalidResponse("missing result"))?
            .take();
        Ok(serde_json::from_value(result)?)
    }

    pub async fn send(&self, wire: &[u8]) -> Result<B256, ClientError> {
        let expected = NativeTx::decode_wire(wire)
            .map_err(|_| ClientError::InvalidResponse("invalid signed request"))?
            .tx_id()
            .0;
        let returned: B256 = self
            .rpc(
                "hub_sendNativeTx",
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
                "hub_getReceiptProof",
                serde_json::json!([hash]),
                RECEIPT_RESPONSE_BYTES,
            )
            .await?;
        if let Some(response) = &response {
            response.verify(hash, trusted)?;
        }
        Ok(response)
    }

    pub async fn eth_call(&self, to: Address, data: Bytes) -> Result<Bytes, ClientError> {
        self.rpc(
            "eth_call",
            serde_json::json!([{ "to": to, "data": data }, "latest"]),
            4 << 20,
        )
        .await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error("RPC error ({code}): {message}")]
    Rpc { code: i64, message: String },
    #[error("invalid RPC response: {0}")]
    InvalidResponse(&'static str),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Receipt(#[from] hub_domain::ReceiptResponseError),
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

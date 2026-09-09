/// Client for SourceHub ACP queries and CometBFT transaction broadcast.
pub(crate) struct SourceHubClient {
    /// LCD/REST base URL.
    lcd_address: String,
    /// Reusable gRPC channel. Cloning a channel preserves its connection pool.
    #[cfg(not(target_arch = "wasm32"))]
    grpc_channel: tonic::transport::Channel,
    /// Whole-operation budget for gRPC authorization.
    #[cfg(not(target_arch = "wasm32"))]
    request_timeout: std::time::Duration,
    /// CometBFT RPC address for broadcast_tx_sync
    comet_rpc_address: String,
    /// HTTP client for REST queries (configured with per-request timeout)
    http: reqwest::Client,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, PartialEq, prost::Message)]
struct VerifyAccessRequest {
    #[prost(string, tag = "1")]
    policy_id: String,
    #[prost(message, optional, tag = "2")]
    access_request: Option<AccessRequest>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, PartialEq, prost::Message)]
struct AccessRequest {
    #[prost(message, repeated, tag = "1")]
    operations: Vec<Operation>,
    #[prost(message, optional, tag = "2")]
    actor: Option<Actor>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, PartialEq, prost::Message)]
struct Operation {
    #[prost(message, optional, tag = "1")]
    object: Option<Object>,
    #[prost(string, tag = "2")]
    permission: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, PartialEq, prost::Message)]
struct Object {
    #[prost(string, tag = "1")]
    resource: String,
    #[prost(string, tag = "2")]
    id: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, PartialEq, prost::Message)]
struct Actor {
    #[prost(string, tag = "1")]
    id: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, PartialEq, prost::Message)]
struct VerifyAccessResponse {
    #[prost(bool, tag = "1")]
    valid: bool,
}

impl SourceHubClient {
    pub(crate) fn new(
        lcd_address: String,
        grpc_address: String,
        comet_rpc_address: String,
        request_timeout: std::time::Duration,
    ) -> Result<Self, ClientError> {
        let http = reqwest::Client::builder()
            .timeout(request_timeout)
            .build()
            .map_err(ClientError::Http)?;
        #[cfg(not(target_arch = "wasm32"))]
        let grpc_channel =
            tonic::transport::Endpoint::from_shared(normalize_base_url(&grpc_address))
                .map_err(|error| ClientError::QueryFailed(error.to_string()))?
                .connect_lazy();
        #[cfg(target_arch = "wasm32")]
        let _ = grpc_address;
        Ok(Self {
            lcd_address,
            #[cfg(not(target_arch = "wasm32"))]
            grpc_channel,
            #[cfg(not(target_arch = "wasm32"))]
            request_timeout,
            comet_rpc_address,
            http,
        })
    }

    /// Query a policy by ID.
    pub(crate) async fn query_policy(
        &self,
        policy_id: &str,
    ) -> Result<Option<PolicyInfo>, ClientError> {
        let url = format!(
            "{}/sourcenetwork/sourcehub/acp/policy/{}",
            self.rest_base_url(),
            policy_id
        );
        let resp = self.http.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            if text.contains("NOT_FOUND") || text.contains("not found") {
                return Ok(None);
            }
            return Err(ClientError::QueryFailed(text));
        }
        let body: serde_json::Value = resp.json().await?;
        let record = body.get("record").unwrap_or(&body);
        let policy = record.get("policy").unwrap_or(&body["policy"]);
        if policy.is_null() {
            return Ok(None);
        }
        Ok(Some(PolicyInfo {
            id: policy_id.to_string(),
            name: policy["name"].as_str().unwrap_or("").to_string(),
            raw_policy: record["raw_policy"].as_str().map(ToOwned::to_owned),
        }))
    }

    /// Query the owner of an object registered under a policy.
    /// Returns (is_registered, owner_did).
    pub(crate) async fn query_object_owner(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<(bool, String), ClientError> {
        let url = format!(
            "{}/sourcenetwork/sourcehub/acp/object_owner/{}/{}/{}",
            self.rest_base_url(),
            policy_id,
            resource,
            object_id
        );
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            if text.contains("NOT_FOUND") || text.contains("not found") {
                return Ok((false, String::new()));
            }
            return Err(ClientError::QueryFailed(text));
        }
        let body: serde_json::Value = resp.json().await?;
        let is_registered = body["is_registered"].as_bool().unwrap_or(false);
        let owner_did = body["record"]["relationship"]["subject"]["actor"]["id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok((is_registered, owner_did))
    }

    /// Verify if an actor has access to an object.
    ///
    /// Uses SourceHub's gRPC API because its REST gateway cannot represent
    /// the repeated nested operation without dropping it.
    pub(crate) async fn verify_access(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        permission: &str,
        actor_did: &str,
    ) -> Result<bool, ClientError> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (policy_id, resource, object_id, permission, actor_did);
            return Err(ClientError::QueryFailed(
                "SourceHub access verification requires native gRPC".to_string(),
            ));
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let operation = async {
                let mut grpc = tonic::client::Grpc::new(self.grpc_channel.clone());
                grpc.ready()
                    .await
                    .map_err(|error| ClientError::QueryFailed(error.to_string()))?;
                let request = VerifyAccessRequest {
                    policy_id: policy_id.to_string(),
                    access_request: Some(AccessRequest {
                        operations: vec![Operation {
                            object: Some(Object {
                                resource: resource.to_string(),
                                id: object_id.to_string(),
                            }),
                            permission: permission.to_string(),
                        }],
                        actor: Some(Actor {
                            id: actor_did.to_string(),
                        }),
                    }),
                };
                let response: tonic::Response<VerifyAccessResponse> = grpc
                    .unary(
                        tonic::Request::new(request),
                        tonic::codegen::http::uri::PathAndQuery::from_static(
                            "/sourcehub.acp.Query/VerifyAccessRequest",
                        ),
                        tonic_prost::ProstCodec::default(),
                    )
                    .await
                    .map_err(|error| ClientError::QueryFailed(error.to_string()))?;
                let valid = response.into_inner().valid;
                tracing::debug!(permission, actor_did, valid, "verify_access result");
                Ok(valid)
            };
            tokio::time::timeout(self.request_timeout, operation)
                .await
                .map_err(|_| {
                    ClientError::Timeout(format!(
                        "gRPC access verification exceeded {:?}",
                        self.request_timeout
                    ))
                })?
        }
    }

    /// Query account number and sequence for transaction signing.
    pub(crate) async fn query_account(&self, address: &str) -> Result<(u64, u64), ClientError> {
        let url = format!(
            "{}/cosmos/auth/v1beta1/accounts/{}",
            self.rest_base_url(),
            address
        );
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(ClientError::QueryFailed(
                resp.text().await.unwrap_or_default(),
            ));
        }
        let body: serde_json::Value = resp.json().await?;
        let account = &body["account"];
        let account_number = account["account_number"]
            .as_str()
            .unwrap_or("0")
            .parse::<u64>()
            .unwrap_or(0);
        let sequence = account["sequence"]
            .as_str()
            .unwrap_or("0")
            .parse::<u64>()
            .unwrap_or(0);
        Ok((account_number, sequence))
    }

    /// Broadcast a signed transaction via CometBFT JSON-RPC.
    /// Returns the tx hash on success.
    pub(crate) async fn broadcast_tx_sync(&self, tx_bytes: &[u8]) -> Result<String, ClientError> {
        let tx_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, tx_bytes);
        let url = self.comet_rpc_base_url();
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "broadcast_tx_sync",
            "params": {
                "tx": tx_b64,
            }
        });
        let resp = self.http.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(ClientError::BroadcastFailed(
                resp.text().await.unwrap_or_default(),
            ));
        }
        let result: serde_json::Value = resp.json().await?;
        let code = result["result"]["code"].as_u64().unwrap_or(1);
        if code != 0 {
            let log = result["result"]["log"].as_str().unwrap_or("unknown error");
            return Err(ClientError::TxFailed(format!("code={}: {}", code, log)));
        }
        let hash = result["result"]["hash"].as_str().unwrap_or("").to_string();
        Ok(hash)
    }

    /// Wait for a transaction to be included in a block.
    /// Returns the full CometBFT tx query response on success.
    pub(crate) async fn await_tx(
        &self,
        tx_hash: &str,
        timeout_ms: u64,
    ) -> Result<serde_json::Value, ClientError> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);
        loop {
            if start.elapsed() > timeout {
                return Err(ClientError::TxTimeout(tx_hash.to_string()));
            }
            let url = format!("{}/tx?hash=0x{}", self.comet_rpc_base_url(), tx_hash);
            let resp = self.http.get(&url).send().await;
            if let Ok(r) = resp {
                if r.status().is_success() {
                    let body: serde_json::Value = r.json().await.unwrap_or(serde_json::Value::Null);
                    let code = body["result"]["tx_result"]["code"].as_u64();
                    if let Some(0) = code {
                        return Ok(body);
                    }
                    if let Some(c) = code {
                        let log = body["result"]["tx_result"]["log"]
                            .as_str()
                            .unwrap_or("unknown");
                        return Err(ClientError::TxFailed(format!(
                            "tx execution failed: code={} log={}",
                            c, log
                        )));
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    pub(crate) fn sequence_cache_key(&self, address: &str) -> String {
        format!("{}::{}", self.comet_rpc_address, address)
    }

    /// Normalize the configured LCD base URL.
    fn rest_base_url(&self) -> String {
        let addr = &self.lcd_address;
        if addr.starts_with("http") {
            addr.clone()
        } else if let Some(rest) = addr.strip_prefix("tcp://") {
            format!("http://{}", rest)
        } else {
            format!("http://{}", addr)
        }
    }

    fn comet_rpc_base_url(&self) -> String {
        let addr = &self.comet_rpc_address;
        if addr.starts_with("http") {
            addr.clone()
        } else if let Some(rest) = addr.strip_prefix("tcp://") {
            format!("http://{}", rest)
        } else {
            format!("http://{}", addr)
        }
    }
}

fn normalize_base_url(address: &str) -> String {
    if address.starts_with("http") {
        address.to_string()
    } else if let Some(rest) = address.strip_prefix("tcp://") {
        format!("http://{rest}")
    } else {
        format!("http://{address}")
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PolicyInfo {
    pub id: String,
    pub name: String,
    pub raw_policy: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ClientError {
    #[error("HTTP request failed: {0}")]
    Http(reqwest::Error),

    #[error("query failed: {0}")]
    QueryFailed(String),

    #[error("broadcast failed: {0}")]
    BroadcastFailed(String),

    #[error("transaction failed: {0}")]
    TxFailed(String),

    #[error("transaction timeout waiting for hash: {0}")]
    TxTimeout(String),

    #[error("SourceHub unreachable (timeout): {0}")]
    Timeout(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<reqwest::Error> for ClientError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() || e.is_connect() {
            ClientError::Timeout(e.to_string())
        } else {
            ClientError::Http(e)
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::time::{Duration, Instant};

    use tokio::net::TcpListener;

    use super::{ClientError, SourceHubClient};

    #[tokio::test]
    async fn grpc_access_verification_obeys_request_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("stalled gRPC server should bind");
        let address = format!(
            "http://{}",
            listener
                .local_addr()
                .expect("stalled gRPC server should have an address")
        );
        tokio::spawn(async move {
            let _connection = listener
                .accept()
                .await
                .expect("stalled gRPC server should accept");
            std::future::pending::<()>().await;
        });
        let timeout = Duration::from_millis(50);
        let client = SourceHubClient::new(address.clone(), address.clone(), address, timeout)
            .expect("client should build");

        let started = Instant::now();
        let result = client
            .verify_access("policy", "resource", "object", "read", "did:key:zactor")
            .await;

        assert!(matches!(result, Err(ClientError::Timeout(_))));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}

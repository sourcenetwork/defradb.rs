use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::provider::{
    AcpLightClientStatus, ProviderError, ProviderPolicyInfo, SourceHubProvider, SubjectRef,
};
use crate::tuning::AcpTuning;
use acp_light_client::AcpLightClient;
use alloy_primitives::{Bytes, FixedBytes};
use alloy_sol_types::{SolCall, SolEvent};
use async_trait::async_trait;
use commonware_codec::DecodeExt as _;
use events::{AcpCacheInvalidatedData, AcpHeightAdvancedData, Bus, Message};
use hub_domain::ConsensusPublicKey;
use hub_modules::acp::types::PolicyRecord;
use k256::ecdsa::SigningKey;
use serde::Deserialize;

use super::abi::{IAcp, ACP_ADDRESS};
use super::bearer;
use super::client::HubRsClient;
use super::provider_commands::{
    encode_archive_object_cmd, encode_delete_relationship_cmd, encode_register_object_cmd,
    encode_set_relationship_cmd, resolve_registered_or_passthrough_bearer_token,
};
use super::worker::NativeWorker;

mod submission;

pub struct HubRsProvider {
    light_client: Arc<AcpLightClient>,
    client: HubRsClient,
    worker: Arc<tokio::sync::Mutex<NativeWorker>>,
    worker_did: String,
    actor_did: String,
    deployment: u64,
    trusted: ConsensusPublicKey,
    signing_key: SigningKey,
    sync_timeout: Duration,
    light_client_observability: Arc<AtomicU64>,
    light_client_observer_handle: tokio::task::JoinHandle<()>,
}

fn derive_ws_url(rpc_url: &str) -> String {
    rpc_url
        .replacen("http://", "ws://", 1)
        .replacen("https://", "wss://", 1)
}

impl HubRsProvider {
    pub async fn new(
        rpc_url: String,
        trusted_consensus_key: &str,
        private_key: &[u8],
        worker: NativeWorker,
        tuning: &AcpTuning,
        event_bus: Option<Arc<dyn Bus>>,
    ) -> Result<Self, ProviderError> {
        let encoded = hex::decode(
            trusted_consensus_key
                .strip_prefix("0x")
                .unwrap_or(trusted_consensus_key),
        )
        .map_err(|e| ProviderError::Config(format!("invalid trusted consensus key: {e}")))?;
        let trusted = ConsensusPublicKey::decode(encoded.as_slice())
            .map_err(|e| ProviderError::Config(format!("invalid trusted consensus key: {e}")))?;
        let ws_url = derive_ws_url(&rpc_url);
        let light_client = Arc::new(
            AcpLightClient::new(&rpc_url, &ws_url, trusted_consensus_key, 10)
                .await
                .map_err(|e| ProviderError::Config(format!("light client: {}", e)))?,
        );
        light_client
            .wait_for_height(1, tuning.receipt_timeout)
            .await
            .map_err(|e| ProviderError::Config(format!("initial verified state: {e}")))?;
        let light_client_observability = Arc::new(AtomicU64::new(0));

        let client = HubRsClient::new(rpc_url, tuning.request_timeout)
            .map_err(|e| ProviderError::Config(format!("HTTP client: {e}")))?;
        let signing_key = SigningKey::from_slice(private_key)
            .map_err(|e| ProviderError::Config(format!("actor key: {e}")))?;
        let actor_did = bearer::did_from_signing_key(&signing_key, true);
        let worker_did = worker.did().to_owned();
        let deployment = worker.deployment_id();

        let light_client_observer_handle = tokio::spawn(run_light_client_observer(
            light_client.clone(),
            light_client_observability.clone(),
            event_bus,
        ));

        let provider = Self {
            light_client,
            client,
            worker: Arc::new(tokio::sync::Mutex::new(worker)),
            worker_did,
            actor_did,
            deployment,
            trusted,
            signing_key,
            sync_timeout: tuning.receipt_timeout,
            light_client_observability,
            light_client_observer_handle,
        };
        provider.recover_pending().await?;
        Ok(provider)
    }

    async fn query_policy_raw(&self, policy_id: &str) -> Result<Option<String>, ProviderError> {
        let record = self
            .light_client
            .read_policy(policy_id)
            .await
            .map_err(|e| ProviderError::Query(format!("policy proof: {e}")))?;
        let Some(bytes) = record.value.as_deref() else {
            return Ok(None);
        };
        let record: HubRsPolicyRecord = serde_json::from_slice(bytes)
            .map_err(|e| ProviderError::Query(format!("policy JSON: {e}")))?;
        Ok(record.raw_policy)
    }

    fn policy_id_to_bytes32(policy_id: &str) -> Result<FixedBytes<32>, ProviderError> {
        hub_client::parse_policy_id(policy_id).map_err(|e| ProviderError::Query(e.to_string()))
    }
}

fn access_request(
    resource: &str,
    object_id: &str,
    permission: &str,
    actor_did: &str,
) -> Result<acp_light_client::AccessRequest, ProviderError> {
    let actor = actor_did
        .parse()
        .map_err(|e| ProviderError::Query(format!("actor DID: {e}")))?;
    Ok(acp_light_client::AccessRequest {
        actor: acp_light_client::Actor(actor),
        operations: vec![acp_light_client::Operation {
            object: acp_light_client::Object {
                resource: resource.into(),
                id: object_id.into(),
            },
            permission: permission.into(),
        }],
    })
}

#[derive(Deserialize)]
struct HubRsPolicyRecord {
    raw_policy: Option<String>,
}

impl Drop for HubRsProvider {
    fn drop(&mut self) {
        self.light_client_observer_handle.abort();
    }
}

fn format_root(root: alloy_primitives::B256) -> String {
    format!("0x{}", hex::encode(root))
}

async fn run_light_client_observer(
    light_client: Arc<AcpLightClient>,
    last_invalidation_height: Arc<AtomicU64>,
    event_bus: Option<Arc<dyn Bus>>,
) {
    let mut next_height = 1u64;
    let mut previous_root = None;

    loop {
        let sync = match light_client
            .wait_for_height(next_height, Duration::from_secs(24 * 60 * 60))
            .await
        {
            Ok(sync) => sync,
            Err(error) => {
                tracing::debug!(error = %error, "ACP light client observer wait_for_height failed");
                next_height = light_client
                    .header_chain()
                    .latest_height()
                    .saturating_add(1);
                continue;
            }
        };

        let module_state_root = format_root(sync.module_state_root);
        if let Some(ref bus) = event_bus {
            bus.publish(Message::acp_height_advanced(AcpHeightAdvancedData {
                height: sync.height,
                module_state_root: module_state_root.clone(),
            }));
        }

        if let Some(previous_root_value) = previous_root {
            if previous_root_value != sync.module_state_root {
                let entries_invalidated = light_client
                    .cache()
                    .invalidate_stale(sync.module_state_root);
                last_invalidation_height.store(sync.height, Ordering::Relaxed);

                if let Some(ref bus) = event_bus {
                    bus.publish(Message::acp_cache_invalidated(AcpCacheInvalidatedData {
                        height: sync.height,
                        module_state_root: module_state_root.clone(),
                        previous_root: format_root(previous_root_value),
                        entries_invalidated,
                    }));
                }
            }
        }

        previous_root = Some(sync.module_state_root);
        next_height = sync.height.saturating_add(1);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl SourceHubProvider for HubRsProvider {
    fn authorized_account(&self) -> String {
        self.worker_did.clone()
    }

    async fn create_bearer_token(&self, did: &str) -> Result<String, ProviderError> {
        if Some(did.to_string()) == self.self_did() {
            return bearer::create_bearer_token(
                &self.signing_key,
                did,
                &self.worker_did,
                self.deployment,
                300,
            )
            .map_err(|e| {
                ProviderError::Config(format!("node bearer token creation failed: {}", e))
            });
        }

        if let Some(token) =
            resolve_registered_or_passthrough_bearer_token(did, &self.worker_did, self.deployment)?
        {
            return Ok(token);
        }

        tracing::warn!(
            did,
            "hub.rs bearer token creation failed: no signing config for DID and no request token"
        );
        Err(ProviderError::Config(format!(
            "no signing config found for DID: {}",
            did
        )))
    }

    fn self_did(&self) -> Option<String> {
        Some(self.actor_did.clone())
    }

    async fn create_policy(&self, policy_yaml: &str) -> Result<String, ProviderError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| ProviderError::Config(e.to_string()))?
            .as_secs();
        let token = hub_client::create_scoped_bearer_token(
            &self.signing_key,
            &self.worker_did,
            self.deployment,
            now,
            now.checked_add(300)
                .ok_or_else(|| ProviderError::Config("invalid clock".into()))?,
            hub_client::DelegationScope::CreatePolicy,
        )
        .map_err(|e| ProviderError::Config(e.to_string()))?;
        let call = IAcp::bearerCreatePolicyCall {
            bearerToken: token,
            policy: policy_yaml.as_bytes().to_vec().into(),
            marshalType: 1,
        };
        let confirmed = self.send_tx_with_sequence(call.abi_encode().into()).await?;
        let mut events = confirmed.receipt.logs().iter().filter(|log| {
            log.address == ACP_ADDRESS
                && log.topics().first() == Some(&IAcp::DelegatedPolicyCreated::SIGNATURE_HASH)
        });
        let log = events
            .next()
            .ok_or_else(|| ProviderError::Query("creation event missing".into()))?;
        let event = IAcp::DelegatedPolicyCreated::decode_raw_log_validate(
            log.topics().iter().copied(),
            &log.data.data,
        )
        .map_err(|e| ProviderError::Query(format!("creation event: {e}")))?;
        if events.next().is_some() || event.creator != self.actor_did {
            return Err(ProviderError::Query(
                "creation event does not match the actor".into(),
            ));
        }
        let id = hex::encode(event.policyId);
        let record = self
            .light_client
            .read_policy(&id)
            .await
            .map_err(|e| ProviderError::Query(format!("created policy proof: {e}")))?;
        let record: PolicyRecord = serde_json::from_slice(
            record
                .value
                .as_deref()
                .ok_or_else(|| ProviderError::Query("created policy absent".into()))?,
        )
        .map_err(|e| ProviderError::Query(format!("created policy record: {e}")))?;
        if record.policy.id != id
            || record.raw_policy != policy_yaml
            || record.metadata.owner_did != self.actor_did
            || record.metadata.tx_signer != self.worker_did
            || record.metadata.tx_hash != confirmed.receipt.tx_hash.as_slice()
            || record.metadata.creation_ts.block_height != confirmed.revision
        {
            return Err(ProviderError::Query(
                "created policy does not match the signed request".into(),
            ));
        }
        Ok(id)
    }

    async fn register_object(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<(), ProviderError> {
        let pid = Self::policy_id_to_bytes32(policy_id)?;
        let cmd = encode_register_object_cmd(resource, object_id);
        let call = IAcp::bearerPolicyCmdCall {
            bearerToken: bearer_token.to_string(),
            policyId: pid,
            cmd: Bytes::from(cmd),
        };
        let calldata = Bytes::from(call.abi_encode());
        let send_tx_start = Instant::now();
        self.send_tx(calldata).await?;
        tracing::info!(
            doc_id = %object_id,
            elapsed = ?send_tx_start.elapsed(),
            "hub.rs register_object send_tx completed"
        );

        Ok(())
    }

    async fn archive_object(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<(), ProviderError> {
        let pid = Self::policy_id_to_bytes32(policy_id)?;
        let cmd = encode_archive_object_cmd(resource, object_id);
        let call = IAcp::bearerPolicyCmdCall {
            bearerToken: bearer_token.to_string(),
            policyId: pid,
            cmd: Bytes::from(cmd),
        };
        let calldata = Bytes::from(call.abi_encode());
        self.send_tx(calldata).await?;

        Ok(())
    }

    async fn set_relationship(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        let pid = Self::policy_id_to_bytes32(policy_id)?;
        let cmd = encode_set_relationship_cmd(resource, object_id, relation, subject);
        let call = IAcp::bearerPolicyCmdCall {
            bearerToken: bearer_token.to_string(),
            policyId: pid,
            cmd: Bytes::from(cmd),
        };
        let calldata = Bytes::from(call.abi_encode());
        self.send_tx(calldata).await?;

        Ok(true)
    }

    async fn delete_relationship(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        let pid = Self::policy_id_to_bytes32(policy_id)?;
        let cmd = encode_delete_relationship_cmd(resource, object_id, relation, subject);
        let call = IAcp::bearerPolicyCmdCall {
            bearerToken: bearer_token.to_string(),
            policyId: pid,
            cmd: Bytes::from(cmd),
        };
        let calldata = Bytes::from(call.abi_encode());
        self.send_tx(calldata).await?;

        Ok(true)
    }

    async fn set_relationship_subject(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        kind: u8,
        subject_resource: &str,
        subject_object_id: &str,
        subject_relation: &str,
    ) -> Result<bool, ProviderError> {
        let subject =
            zanzibar::decode_subject(kind, subject_resource, subject_object_id, subject_relation)
                .map_err(|e| ProviderError::Config(format!("relationship subject: {e}")))?;
        let cmd = serde_json::to_vec(&serde_json::json!({
            "SetRelationship": { "resource": resource, "object_id": object_id, "relation": relation, "subject": subject }
        })).map_err(|e| ProviderError::Config(e.to_string()))?;
        let call = IAcp::bearerPolicyCmdCall {
            bearerToken: self.create_bearer_token(&self.actor_did).await?,
            policyId: Self::policy_id_to_bytes32(policy_id)?,
            cmd: cmd.into(),
        };
        self.send_tx(call.abi_encode().into()).await?;
        Ok(true)
    }

    async fn delete_relationship_subject(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        kind: u8,
        subject_resource: &str,
        subject_object_id: &str,
        subject_relation: &str,
    ) -> Result<bool, ProviderError> {
        let subject =
            zanzibar::decode_subject(kind, subject_resource, subject_object_id, subject_relation)
                .map_err(|e| ProviderError::Config(format!("relationship subject: {e}")))?;
        let cmd = serde_json::to_vec(&serde_json::json!({
            "DeleteRelationship": { "resource": resource, "object_id": object_id, "relation": relation, "subject": subject }
        })).map_err(|e| ProviderError::Config(e.to_string()))?;
        let call = IAcp::bearerPolicyCmdCall {
            bearerToken: self.create_bearer_token(&self.actor_did).await?,
            policyId: Self::policy_id_to_bytes32(policy_id)?,
            cmd: cmd.into(),
        };
        self.send_tx(call.abi_encode().into()).await?;
        Ok(true)
    }

    async fn query_policy(
        &self,
        policy_id: &str,
    ) -> Result<Option<ProviderPolicyInfo>, ProviderError> {
        if let Some(raw_policy) = self.query_policy_raw(policy_id).await? {
            Ok(Some(ProviderPolicyInfo {
                id: policy_id.to_string(),
                name: policy_id.to_string(),
                raw_policy: Some(raw_policy),
            }))
        } else {
            Ok(None)
        }
    }

    async fn query_object_owner(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<(bool, String), ProviderError> {
        let policy = hex::encode(Self::policy_id_to_bytes32(policy_id)?);
        let object = acp_light_client::Object {
            resource: resource.into(),
            id: object_id.into(),
        };
        let owner = self
            .light_client
            .read_object_owner(&policy, &object)
            .await
            .map_err(|e| ProviderError::Query(format!("owner proof: {e}")))?;
        Ok(match owner {
            Some(owner) => (true, owner.0.as_str().into()),
            None => (false, String::new()),
        })
    }

    async fn verify_access(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        permission: &str,
        actor_did: &str,
    ) -> Result<bool, ProviderError> {
        let request = access_request(resource, object_id, permission, actor_did)?;
        self.light_client
            .verify_access(policy_id, &request)
            .await
            .map_err(|e| ProviderError::Query(format!("verified permission: {e}")))
    }

    async fn create_access_decision(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        permission: &str,
        actor_did: &str,
    ) -> Result<Option<String>, ProviderError> {
        let request = access_request(resource, object_id, permission, actor_did)?;
        let call = IAcp::checkAccessCall {
            policyId: Self::policy_id_to_bytes32(policy_id)?,
            resources: vec![resource.to_string()],
            objectIds: vec![object_id.to_string()],
            permissions: vec![permission.to_string()],
            actor: actor_did.to_string(),
        };
        let confirmed = self
            .send_tx_with_sequence(Bytes::from(call.abi_encode()))
            .await?;
        let expected = acp_light_client::DecisionRequest {
            deployment_id: self.deployment,
            policy_id: policy_id.into(),
            creator: self.worker_did.clone(),
            creator_sequence: confirmed.sequence,
            request,
        };
        let decision = self
            .light_client
            .verify_access_decision(&expected)
            .await
            .map_err(|e| ProviderError::Query(format!("verified access decision: {e}")))?;
        Ok(Some(decision.id))
    }

    fn acp_light_client_status(&self) -> Result<AcpLightClientStatus, ProviderError> {
        let sync = self.light_client.header_chain().state();
        let last_invalidation_height = self.light_client_observability.load(Ordering::Relaxed);

        Ok(AcpLightClientStatus {
            height: sync.as_ref().map_or(0, |state| state.height),
            module_state_root: sync
                .as_ref()
                .map_or_else(String::new, |state| format_root(state.module_state_root)),
            cache_entries: self.light_client.cache().len(),
            last_invalidation_height,
            connected: sync.is_some(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::PrivateKey;
    use identity::Identity;

    #[tokio::test]
    async fn trusted_consensus_key_is_required_before_connecting() {
        let root = tempfile::tempdir().unwrap();
        let keys = keyring::FileKeyring::open(root.path().join("keys"), b"test").unwrap();
        for (index, key) in ["", "not-hex", "00"].into_iter().enumerate() {
            let worker =
                NativeWorker::open(&root.path().join(index.to_string()), &keys, 9001).unwrap();
            let error = HubRsProvider::new(
                "http://127.0.0.1:1".into(),
                key,
                &[],
                worker,
                &AcpTuning::default(),
                None,
            )
            .await
            .err()
            .expect("invalid key must reject configuration");
            assert!(
                matches!(error, ProviderError::Config(ref message) if message.contains("trusted consensus key"))
            );
        }
    }

    fn store_remote_secp256r1_identity(did: &str) {
        let private_key = crypto::generate_secp256r1().expect("should generate secp256r1 key");
        let public_key = private_key.public_key();
        let public_key_bytes = public_key.raw_owned();
        let public_key_hex = hex::encode(&public_key_bytes);
        defra_core::signing::store_identity(
            did,
            defra_core::signing::SigningConfig {
                key_type: defra_core::signing::SigningKeyType::Secp256r1,
                private_key_bytes: Vec::new(),
                public_key_bytes,
                public_key_hex,
                remote_signer: None,
                signing_authorization: None,
            },
        );
    }

    #[test]
    fn resolve_registered_or_passthrough_bearer_token_uses_request_token_for_remote_identity() {
        let _guard = crate::signing_state_test_guard();
        let did = "did:key:zRemoteHubRsToken";
        let token = "device.jwt.token".to_string();
        defra_core::signing::clear_identity_store();
        defra_core::signing::clear_request_bearer_token(did);

        store_remote_secp256r1_identity(did);
        defra_core::signing::set_request_bearer_token(did, token.clone());

        let resolved =
            resolve_registered_or_passthrough_bearer_token(did, "did:key:submitter", 9001)
                .expect("resolution should succeed")
                .expect("token should resolve");
        assert_eq!(resolved, token);

        defra_core::signing::clear_request_bearer_token(did);
        defra_core::signing::clear_identity_store();
    }

    #[test]
    fn resolve_registered_or_passthrough_bearer_token_builds_local_secp256k1_token() {
        let _guard = crate::signing_state_test_guard();
        let private_key = crypto::generate_secp256k1().expect("should generate secp256k1 key");
        let raw_identity =
            identity::RawIdentity::from_secp256k1(private_key).expect("identity should build");
        let did = raw_identity.did().expect("did should derive").to_string();

        defra_core::signing::clear_identity_store();
        defra_core::signing::store_identity(
            &did,
            defra_core::signing::SigningConfig {
                key_type: defra_core::signing::SigningKeyType::Secp256k1,
                private_key_bytes: defra_core::signing::SigningConfig::private_key_bytes_from_vec(
                    raw_identity.private_key_bytes().to_vec(),
                ),
                public_key_bytes: raw_identity.public_key_bytes().to_vec(),
                public_key_hex: hex::encode(raw_identity.public_key_bytes()),
                remote_signer: None,
                signing_authorization: None,
            },
        );

        let resolved =
            resolve_registered_or_passthrough_bearer_token(&did, "did:key:submitter", 9001)
                .expect("resolution should succeed")
                .expect("token should resolve");
        use base64::Engine;
        let payload = resolved.split('.').nth(1).expect("token payload");
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .expect("payload base64");
        let claims: serde_json::Value = serde_json::from_slice(&payload).expect("claims");
        assert_eq!(claims["iss"], did);
        assert_eq!(claims["sub"], "did:key:submitter");
        assert_eq!(claims["aud"], "vera:9001");
        assert_eq!(claims["scope"], "acp:policy");

        defra_core::signing::clear_request_bearer_token(&did);
        defra_core::signing::clear_identity_store();
    }
}

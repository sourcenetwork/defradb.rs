use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::provider::{
    AcpLightClientStatus, ActorRelationshipQuery, AssignmentQuery, CertifiedAssignments,
    ProviderError, ProviderPolicyInfo, SubjectRef, VeraProvider,
};
use crate::tuning::AcpTuning;
use acp_light_client::{
    AcpLightClient, AcpTransport, FreshnessPolicy, HttpWsTransport, ProofClient,
};
use alloy_primitives::{Bytes, FixedBytes, B256};
use alloy_sol_types::SolCall;
use async_trait::async_trait;
use commonware_codec::DecodeExt as _;
use events::{AcpCacheInvalidatedData, AcpHeightAdvancedData, Bus, Message};
use vera_domain::ConsensusPublicKey;
use vera_modules::acp::types::PolicyRecord;

use super::abi::{IAcp, ACP_ADDRESS};
use super::bearer;
use super::client::{HttpTransport, VeraRsClient};
use super::did_spelling;
use super::follow::{Follow, ProviderChain};
use super::policy_cmd;
#[cfg(test)]
use super::provider_commands::resolve_registered_or_passthrough_bearer_token;
use super::provider_commands::{
    holds_registered_key, resolve_registered_or_passthrough_bearer_token_as,
};
use super::receipt_wait::Fallback;
use super::request_digest;
use super::signer::{admission, DelegationSigner, Secp256k1Signer};
use super::transport::RpcTransport;
use super::worker::NativeWorker;

mod assignments;
#[cfg(test)]
mod audience_tests;
mod create_policy;
mod exact_relationship;
mod genesis;
mod owed_request;
mod reach;
#[cfg(test)]
#[path = "provider/recovery_tests.rs"]
mod recovery_tests;
mod registration_list;
#[cfg(test)]
mod signer_tests;
mod submission;
mod terms;
#[cfg(test)]
mod transport_tests;
mod verified;

pub use genesis::GenesisSource;
use terms::Follower;
use verified::verified;

pub struct VeraRsProvider {
    light_client: Arc<AcpLightClient>,
    client: Arc<VeraRsClient>,
    worker: Arc<regolith::sync::Mutex<NativeWorker>>,
    /// Where the provider stands with the deployment's terms.
    follower: Arc<Follower>,
    worker_did: String,
    actor_did: String,
    deployment: u64,
    /// The deployment's genesis identity, once the light client has proven
    /// the first block.
    genesis_id: Arc<regolith::sync::OnceCell<[u8; 32]>>,
    trusted: ConsensusPublicKey,
    signer: Arc<dyn DelegationSigner>,
    sync_timeout: Duration,
    /// Longest wait between two asks for a receipt with no new header.
    receipt_fallback: Arc<Fallback>,
    light_client_observability: Arc<AtomicU64>,
    _light_client_observer: edge::Service,
    edge: edge::Edge,
}

fn derive_ws_url(rpc_url: &str) -> String {
    rpc_url
        .replacen("http://", "ws://", 1)
        .replacen("https://", "wss://", 1)
}

fn trusted_key(encoded: &str) -> Result<ConsensusPublicKey, ProviderError> {
    let encoded = hex::decode(encoded.strip_prefix("0x").unwrap_or(encoded))
        .map_err(|e| ProviderError::Config(format!("invalid trusted consensus key: {e}")))?;
    ConsensusPublicKey::decode(encoded)
        .map_err(|e| ProviderError::Config(format!("invalid trusted consensus key: {e}")))
}

/// Who a provider acts as: a secp256k1 key it is given, or a signer for a key
/// held elsewhere.
enum Actor<'a> {
    Key(&'a [u8]),
    Signer(Arc<dyn DelegationSigner>),
}

impl Actor<'_> {
    fn signer(self) -> Result<Arc<dyn DelegationSigner>, ProviderError> {
        match self {
            Self::Key(private_key) => Secp256k1Signer::from_slice(private_key)
                .map(|signer| Arc::new(signer) as Arc<dyn DelegationSigner>)
                .map_err(|e| ProviderError::Config(format!("actor key: {e}"))),
            Self::Signer(signer) => Ok(signer),
        }
    }
}

impl VeraRsProvider {
    /// A provider of the node at `rpc_url`, reached over HTTP and, for
    /// finalized headers, the WebSocket at the same address. Its connections,
    /// light client observer and worker calls run on `edge`.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        rpc_url: String,
        trusted_consensus_key: &str,
        genesis: GenesisSource,
        private_key: &[u8],
        worker: NativeWorker,
        tuning: &AcpTuning,
        event_bus: Option<Arc<dyn Bus>>,
        edge: &edge::Edge,
    ) -> Result<Self, ProviderError> {
        // A bad key is reported before either client is built.
        trusted_key(trusted_consensus_key)?;
        let acp = HttpWsTransport::new(&rpc_url, &derive_ws_url(&rpc_url))
            .map_err(|e| ProviderError::Config(format!("light client: {e}")))?;
        let rpc = HttpTransport::new(rpc_url, tuning.request_timeout, edge)
            .map_err(|e| ProviderError::Config(format!("HTTP client: {e}")))?;
        Self::with_transport(
            Arc::new(rpc),
            Arc::new(acp),
            trusted_consensus_key,
            genesis,
            private_key,
            worker,
            tuning,
            event_bus,
            edge,
        )
        .await
    }

    /// As [`new`](Self::new), for an actor whose key `signer` holds.
    #[allow(clippy::too_many_arguments)]
    pub async fn new_with_signer(
        rpc_url: String,
        trusted_consensus_key: &str,
        genesis: GenesisSource,
        signer: Arc<dyn DelegationSigner>,
        worker: NativeWorker,
        tuning: &AcpTuning,
        event_bus: Option<Arc<dyn Bus>>,
        edge: &edge::Edge,
    ) -> Result<Self, ProviderError> {
        trusted_key(trusted_consensus_key)?;
        let acp = HttpWsTransport::new(&rpc_url, &derive_ws_url(&rpc_url))
            .map_err(|e| ProviderError::Config(format!("light client: {e}")))?;
        let rpc = HttpTransport::new(rpc_url, tuning.request_timeout, edge)
            .map_err(|e| ProviderError::Config(format!("HTTP client: {e}")))?;
        Self::with_signer(
            Arc::new(rpc),
            Arc::new(acp),
            trusted_consensus_key,
            genesis,
            signer,
            worker,
            tuning,
            event_bus,
            edge,
        )
        .await
    }

    /// A provider that reaches the node through the transports it is given:
    /// `rpc` carries submissions and receipt reads, `acp` the light client's
    /// proof reads and finalized headers. Neither is trusted: every receipt,
    /// proof and header is checked against `trusted_consensus_key`.
    #[allow(clippy::too_many_arguments)]
    pub async fn with_transport(
        rpc: Arc<dyn RpcTransport>,
        acp: Arc<dyn AcpTransport>,
        trusted_consensus_key: &str,
        genesis: GenesisSource,
        private_key: &[u8],
        worker: NativeWorker,
        tuning: &AcpTuning,
        event_bus: Option<Arc<dyn Bus>>,
        edge: &edge::Edge,
    ) -> Result<Self, ProviderError> {
        Self::connect(
            rpc,
            acp,
            trusted_consensus_key,
            genesis,
            Actor::Key(private_key),
            worker,
            tuning,
            event_bus,
            edge,
        )
        .await
    }

    /// As [`with_transport`](Self::with_transport), for an actor whose key
    /// `signer` holds. The provider builds each delegation token itself and
    /// sends one only when `signer`'s signature verifies under its DID's key.
    #[allow(clippy::too_many_arguments)]
    pub async fn with_signer(
        rpc: Arc<dyn RpcTransport>,
        acp: Arc<dyn AcpTransport>,
        trusted_consensus_key: &str,
        genesis: GenesisSource,
        signer: Arc<dyn DelegationSigner>,
        worker: NativeWorker,
        tuning: &AcpTuning,
        event_bus: Option<Arc<dyn Bus>>,
        edge: &edge::Edge,
    ) -> Result<Self, ProviderError> {
        Self::connect(
            rpc,
            acp,
            trusted_consensus_key,
            genesis,
            Actor::Signer(signer),
            worker,
            tuning,
            event_bus,
            edge,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn connect(
        rpc: Arc<dyn RpcTransport>,
        acp: Arc<dyn AcpTransport>,
        trusted_consensus_key: &str,
        genesis: GenesisSource,
        actor: Actor<'_>,
        worker: NativeWorker,
        tuning: &AcpTuning,
        event_bus: Option<Arc<dyn Bus>>,
        edge: &edge::Edge,
    ) -> Result<Self, ProviderError> {
        let trusted = trusted_key(trusted_consensus_key)?;
        let genesis_id = Arc::new(regolith::sync::OnceCell::new());
        let light_client = edge::compat(async {
            let named = match genesis {
                GenesisSource::Configured(id) => Some(id),
                GenesisSource::Hosted => {
                    let proofs = ProofClient::with_transport(acp.clone(), trusted_consensus_key)
                        .map_err(|e| ProviderError::Config(format!("light client: {e}")))?;
                    Some(
                        genesis::read_within(&proofs, &trusted, tuning.receipt_timeout)
                            .await
                            .map_err(|e| {
                                ProviderError::Config(format!("hosted genesis identity: {e}"))
                            })?,
                    )
                }
                GenesisSource::Unnamed => {
                    tracing::warn!(
                        "no Vera genesis identifier is configured: after a switch of the \
                         deployment's active device every verified read fails until one is"
                    );
                    None
                }
            };
            let light_client = match named {
                Some(id) => {
                    let _ = genesis_id.set(id);
                    AcpLightClient::for_deployment(
                        acp,
                        trusted_consensus_key,
                        B256::from(id),
                        10,
                        FreshnessPolicy::default(),
                    )
                }
                None => AcpLightClient::with_transport(
                    acp,
                    trusted_consensus_key,
                    10,
                    FreshnessPolicy::default(),
                ),
            }
            .map_err(|e| ProviderError::Config(format!("light client: {}", e)))?;
            let light_client = Arc::new(light_client);
            light_client
                .wait_for_height(1, tuning.receipt_timeout)
                .await
                .map_err(|e| ProviderError::Config(format!("initial verified state: {e}")))?;
            match genesis::read(light_client.proof_client(), &trusted).await {
                Ok(served) => match named {
                    Some(id) if id != served => {
                        return Err(ProviderError::Config(format!(
                            "the Vera node serves the deployment whose genesis is 0x{}, not \
                             the configured 0x{}",
                            hex::encode(served),
                            hex::encode(id)
                        )))
                    }
                    Some(_) => {}
                    None => {
                        let _ = genesis_id.set(served);
                    }
                },
                Err(error) => {
                    tracing::warn!(error = %error, "vera.rs genesis identity not proven at start")
                }
            }
            Ok::<_, ProviderError>(light_client)
        })
        .await?;
        let light_client_observability = Arc::new(AtomicU64::new(0));
        let receipt_fallback = Arc::new(Fallback::new(tuning.receipt_fallback));

        let client = Arc::new(VeraRsClient::with_transport(rpc));
        let signer = actor.signer()?;
        admission::actor_key(signer.as_ref())
            .map_err(|e| ProviderError::Config(format!("actor signer: {e}")))?;
        let actor_did = did_spelling::defra(signer.did()).into_owned();
        let worker_did = worker.did().to_owned();
        let deployment = worker.deployment_id();
        let mut worker = worker;
        let follow = Follow::from_journal(worker.confirmed());
        let worker = Arc::new(regolith::sync::Mutex::new(worker));
        let own = actor_did.clone();
        let follower = Arc::new(Follower {
            light_client: light_client.clone(),
            chain: ProviderChain {
                client: client.clone(),
                light_client: light_client.clone(),
                trusted,
                timeout: tuning.receipt_timeout,
                fallback: receipt_fallback.clone(),
            },
            worker: worker.clone(),
            follow,
            bus: event_bus.clone(),
            sink: regolith::sync::OnceCell::new(),
            recovery: regolith::sync::OnceCell::new(),
            shared: std::sync::atomic::AtomicBool::new(false),
            holds_key: Box::new(move |did| did == own || holds_registered_key(did)),
        });

        let _light_client_observer = edge.spawn_service(edge::compat(run_light_client_observer(
            light_client.clone(),
            light_client_observability.clone(),
            event_bus,
            Genesis {
                trusted,
                id: genesis_id.clone(),
            },
        )));

        let provider = Self {
            light_client,
            client,
            worker,
            follower,
            worker_did,
            actor_did,
            deployment,
            genesis_id,
            trusted,
            signer,
            sync_timeout: tuning.receipt_timeout,
            receipt_fallback,
            light_client_observability,
            _light_client_observer,
            edge: edge.clone(),
        };
        provider.follower.check_with(&provider).await;
        provider.recover_pending().await?;
        Ok(provider)
    }

    /// Whether the provider may decide and send now, and the term last
    /// checked: not while a switch of the deployment's active device is
    /// unchecked, nor once the node could not tell what one voided. A switch
    /// not yet checked is checked here, on the caller's task.
    async fn admitted(&self) -> Result<u64, ProviderError> {
        match self.follower.admit() {
            Ok(epoch) => Ok(epoch),
            Err(_) => {
                self.follower.check_with(self).await;
                self.follower.admit()
            }
        }
    }

    /// `read`, refused unless the provider is admitted before it and still
    /// admitted at the same checked term after it: a term certified, or
    /// checked, while the read was on its way may void the state it was
    /// answered at.
    async fn admitted_read<T>(
        &self,
        read: impl std::future::Future<Output = Result<T, ProviderError>>,
    ) -> Result<T, ProviderError> {
        let epoch = self.admitted().await?;
        let answer = read.await?;
        self.follower.still(epoch)?;
        Ok(answer)
    }

    /// How many waits for a receipt ended at
    /// [`AcpTuning::receipt_fallback`] rather than at a new finalized
    /// header, since the provider was built.
    pub fn receipt_fallbacks(&self) -> u64 {
        self.receipt_fallback.expired()
    }

    fn policy_id_to_bytes32(policy_id: &str) -> Result<FixedBytes<32>, ProviderError> {
        vera_client::parse_policy_id(policy_id).map_err(|e| ProviderError::Query(e.to_string()))
    }
}

fn access_request(
    resource: &str,
    object_id: &str,
    permission: &str,
    actor_did: &str,
) -> Result<acp_light_client::AccessRequest, ProviderError> {
    let actor = did_spelling::wire(actor_did)
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

fn format_root(root: alloy_primitives::B256) -> String {
    format!("0x{}", hex::encode(root))
}

/// Where the provider keeps the deployment's genesis identity, and the key
/// that proves it.
struct Genesis {
    trusted: ConsensusPublicKey,
    id: Arc<regolith::sync::OnceCell<[u8; 32]>>,
}

/// Reads the deployment's genesis identity into `known` unless it is there.
/// A read that fails leaves it unknown: request-bound delegations are then
/// checked without it, as by a provider that has none, and the light client
/// observer asks again at each new height.
async fn learn_genesis(
    light_client: &AcpLightClient,
    trusted: &ConsensusPublicKey,
    known: &regolith::sync::OnceCell<[u8; 32]>,
) -> Result<(), ProviderError> {
    if known.get().is_none() {
        let _ = known.set(genesis::read(light_client.proof_client(), trusted).await?);
    }
    Ok(())
}

/// How long the observer waits for the next height before it asks the
/// light client for its height again: a term that voids the head lowers the
/// height, so the one it waited for may be long in coming.
const NEXT_HEIGHT: Duration = Duration::from_secs(1);

async fn run_light_client_observer(
    light_client: Arc<AcpLightClient>,
    last_invalidation_height: Arc<AtomicU64>,
    event_bus: Option<Arc<dyn Bus>>,
    genesis: Genesis,
) {
    let mut next_height = 1u64;
    let mut previous_root = None;

    loop {
        let sync = match light_client.wait_for_height(next_height, NEXT_HEIGHT).await {
            Ok(sync) => sync,
            Err(error) => {
                tracing::trace!(error = %error, "ACP light client observer wait_for_height ended");
                next_height = light_client
                    .header_chain()
                    .latest_height()
                    .saturating_add(1);
                continue;
            }
        };

        if let Err(error) = learn_genesis(&light_client, &genesis.trusted, &genesis.id).await {
            tracing::debug!(error = %error, "vera.rs genesis identity not yet proven");
        }
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
impl VeraProvider for VeraRsProvider {
    fn authorized_account(&self) -> String {
        self.worker_did.clone()
    }

    fn archive_object_on_document_delete(&self) -> bool {
        // A local document tombstone may still be rejected by another
        // replica. Archiving the shared ACP object here would revoke grants
        // before the replicated delete has been authorized.
        false
    }

    async fn create_bearer_token(&self, did: &str) -> Result<String, ProviderError> {
        self.create_bearer_token_as(&defra_core::RequestContext::ambient(), did)
            .await
    }

    async fn create_bearer_token_as(
        &self,
        context: &defra_core::RequestContext,
        did: &str,
    ) -> Result<String, ProviderError> {
        if did_spelling::defra(did) == self.actor_did {
            return bearer::create_bearer_token(
                self.signer.as_ref(),
                &did_spelling::wire(&self.actor_did),
                &self.worker_did,
                self.deployment,
                300,
            )
            .await
            .map_err(|e| {
                ProviderError::Config(format!("node bearer token creation failed: {}", e))
            });
        }

        if let Some(token) = resolve_registered_or_passthrough_bearer_token_as(
            context,
            did,
            &self.worker_did,
            self.deployment,
        )
        .await?
        {
            return Ok(token);
        }

        tracing::warn!(
            did,
            "vera.rs bearer token creation failed: no signing config for DID and no request token"
        );
        Err(ProviderError::Config(format!(
            "no signing config found for DID: {}",
            did
        )))
    }

    fn self_did(&self) -> Option<String> {
        Some(self.actor_did.clone())
    }

    fn canonical_actor(&self, did: &str) -> String {
        did_spelling::defra(did).into_owned()
    }

    fn holds_key_for(&self, did: &str) -> bool {
        did_spelling::defra(did) == self.actor_did || holds_registered_key(did)
    }

    fn takes_subject_commands(&self) -> bool {
        true
    }

    /// A submission is confirmed by its verified receipt, so a rejected one
    /// was executed and refused.
    fn write_refused(&self, error: &ProviderError) -> bool {
        matches!(error, ProviderError::Transaction(_))
    }

    fn journals_owed_requests(&self) -> bool {
        true
    }

    fn owed_request(
        &self,
        calls: &[crate::provider::OwedCall<'_>],
    ) -> Result<Vec<u8>, ProviderError> {
        self.prepare_owed_request(calls)
    }

    async fn owed_request_floor(&self) -> Result<u64, ProviderError> {
        self.owed_floor().await
    }

    async fn journal_owed_request(&self, request: &[u8]) -> Result<String, ProviderError> {
        self.journal_owed(request).await
    }

    async fn owed_request_outcome(
        &self,
        digest: &str,
        tx: Option<&str>,
        floor: Option<u64>,
    ) -> Result<crate::provider::OwedRequestOutcome, ProviderError> {
        self.owed_outcome(digest, tx, floor).await
    }

    async fn create_policy(
        &self,
        policy_yaml: &str,
        nonce: Option<acp::PolicyNonce>,
    ) -> Result<String, ProviderError> {
        self.create_policy_with(policy_yaml, nonce).await
    }

    async fn register_object(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<(), ProviderError> {
        let command = crate::PolicyCommand {
            policy_id,
            resource,
            object_id,
            action: crate::PolicyAction::RegisterObject,
        };
        let send_tx_start = Instant::now();
        self.send_command(bearer_token, &command).await?;
        tracing::info!(
            doc_id = %object_id,
            elapsed = ?send_tx_start.elapsed(),
            "vera.rs register_object send_tx completed"
        );

        Ok(())
    }

    async fn register_objects(
        &self,
        bearer_token: &str,
        objects: &[acp::AcpObject<'_>],
    ) -> Result<(), ProviderError> {
        let calldata = registration_list::calldata(bearer_token, objects)?;
        let send_tx_start = Instant::now();
        self.send_tx(calldata).await?;
        tracing::info!(
            objects = objects.len(),
            elapsed = ?send_tx_start.elapsed(),
            "vera.rs register_objects send_tx completed"
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
        let command = policy_cmd::archive_object(resource, object_id);
        self.send_policy_cmd(bearer_token, policy_id, &command)
            .await
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
        let command = crate::PolicyCommand {
            policy_id,
            resource,
            object_id,
            action: crate::PolicyAction::SetRelationship {
                relation,
                subject: crate::PolicySubject::Actor(subject),
            },
        };
        self.send_command(bearer_token, &command).await?;
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
        let command = crate::PolicyCommand {
            policy_id,
            resource,
            object_id,
            action: crate::PolicyAction::DeleteRelationship {
                relation,
                subject: crate::PolicySubject::Actor(subject),
            },
        };
        self.send_command(bearer_token, &command).await?;
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
        let token = self.create_bearer_token(&self.actor_did).await?;
        self.set_relationship_subject_as(
            &token,
            policy_id,
            resource,
            object_id,
            relation,
            kind,
            subject_resource,
            subject_object_id,
            subject_relation,
        )
        .await
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
        let token = self.create_bearer_token(&self.actor_did).await?;
        self.delete_relationship_subject_as(
            &token,
            policy_id,
            resource,
            object_id,
            relation,
            kind,
            subject_resource,
            subject_object_id,
            subject_relation,
        )
        .await
    }

    async fn set_relationship_subject_as(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        kind: u8,
        subject_resource: &str,
        subject_object_id: &str,
        subject_relation: &str,
    ) -> Result<bool, ProviderError> {
        let subject = crate::PolicySubject::Structured {
            kind,
            resource: subject_resource,
            object_id: subject_object_id,
            relation: subject_relation,
        };
        let command = crate::PolicyCommand {
            policy_id,
            resource,
            object_id,
            action: crate::PolicyAction::SetRelationship { relation, subject },
        };
        self.send_command(bearer_token, &command).await?;
        Ok(true)
    }

    async fn delete_relationship_subject_as(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        kind: u8,
        subject_resource: &str,
        subject_object_id: &str,
        subject_relation: &str,
    ) -> Result<bool, ProviderError> {
        let subject = crate::PolicySubject::Structured {
            kind,
            resource: subject_resource,
            object_id: subject_object_id,
            relation: subject_relation,
        };
        let command = crate::PolicyCommand {
            policy_id,
            resource,
            object_id,
            action: crate::PolicyAction::DeleteRelationship { relation, subject },
        };
        self.send_command(bearer_token, &command).await?;
        Ok(true)
    }

    async fn query_policy(
        &self,
        policy_id: &str,
    ) -> Result<Option<ProviderPolicyInfo>, ProviderError> {
        let id = hex::encode(Self::policy_id_to_bytes32(policy_id)?);
        let proof = self
            .admitted_read(async {
                verified(self.light_client.header_chain(), || {
                    self.light_client.read_policy(&id)
                })
                .await
                .map_err(|e| ProviderError::Query(format!("policy proof: {e}")))
            })
            .await?;
        let Some(bytes) = proof.value.as_deref() else {
            return Ok(None);
        };
        let record: PolicyRecord = serde_json::from_slice(bytes)
            .map_err(|e| ProviderError::Query(format!("policy JSON: {e}")))?;
        if record.policy.id != id {
            return Err(ProviderError::Query(
                "policy record identity mismatch".into(),
            ));
        }
        Ok(Some(ProviderPolicyInfo {
            id,
            name: record.policy.name,
            raw_policy: Some(record.raw_policy),
        }))
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
            .admitted_read(async {
                verified(self.light_client.header_chain(), || {
                    self.light_client.read_object_owner(&policy, &object)
                })
                .await
                .map_err(|e| reach::read_error(&self.light_client, "owner proof", e))
            })
            .await?;
        Ok(match owner {
            Some(owner) => (true, did_spelling::defra(owner.0.as_str()).into_owned()),
            None => (false, String::new()),
        })
    }

    async fn read_certified_assignments(
        &self,
        query: AssignmentQuery<'_>,
    ) -> Result<CertifiedAssignments, ProviderError> {
        self.admitted_read(assignments::read(self, query)).await
    }

    async fn query_actor_relationship(
        &self,
        query: ActorRelationshipQuery<'_>,
    ) -> Result<bool, ProviderError> {
        // An observation overtaken by a block between its reads is asked
        // again from the policy read, as every overtaken read is.
        self.admitted_read(async {
            verified(self.light_client.header_chain(), || async {
                exact_relationship::query(&self.light_client, query)
                    .await
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
            })
            .await
            .map_err(|error| match error.downcast::<ProviderError>() {
                Ok(error) => *error,
                Err(error) => ProviderError::Query(error.to_string()),
            })
        })
        .await
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
        self.admitted_read(async {
            verified(self.light_client.header_chain(), || {
                self.light_client.verify_access(policy_id, &request)
            })
            .await
            .map_err(|e| reach::read_error(&self.light_client, "verified permission", e))
        })
        .await
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
            actor: did_spelling::wire(actor_did).into_owned(),
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
            .admitted_read(async {
                verified(self.light_client.header_chain(), || {
                    self.light_client.verify_access_decision(&expected)
                })
                .await
                .map_err(|e| ProviderError::Query(format!("verified access decision: {e}")))
            })
            .await?;
        Ok(Some(decision.id))
    }

    /// Every decision is a permission proof the light client verifies at a
    /// revision no older than `FreshnessPolicy::default().max_age`.
    fn proves_decisions(&self) -> bool {
        true
    }

    /// The state decisions are remembered under, while it is fresh. Once the
    /// light client's newest state is older than its freshness limit there
    /// is none, so no decision made at it is given again: the check goes to
    /// the light client, which refuses a stale state. The state names the
    /// last term checked, so nothing remembered before a switch of the
    /// deployment's active device is given after it, and there is none
    /// while a switch is unchecked.
    fn decision_state(&self) -> Option<String> {
        self.follower.admit().ok()?;
        self.light_client
            .header_chain()
            .fresh_state()
            .ok()
            .map(|state| {
                format!(
                    "{}:{}",
                    self.follower.epoch(),
                    format_root(state.module_state_root)
                )
            })
    }

    fn acp_light_client_status(&self) -> Result<AcpLightClientStatus, ProviderError> {
        let sync = self.light_client.header_chain().state();
        let last_invalidation_height = self.light_client_observability.load(Ordering::Relaxed);

        Ok(AcpLightClientStatus {
            height: sync.as_ref().map_or(0, |state| state.height),
            chain_time: sync.as_ref().map(|state| state.timestamp),
            module_state_root: sync
                .as_ref()
                .map_or_else(String::new, |state| format_root(state.module_state_root)),
            cache_entries: self.light_client.cache().len(),
            last_invalidation_height,
            connected: sync.is_some(),
        })
    }

    /// One outbox per provider. A provider several nodes share cannot tell
    /// whose outbox a voided change belongs in, nor make another node's
    /// owed revocation deny: a term that leaves changes to owe again then
    /// fails closed.
    fn owe_voided_to(&self, sink: std::sync::Weak<dyn acp::OweAgain>) {
        if self.follower.sink.set(sink).is_err() {
            self.follower
                .shared
                .store(true, std::sync::atomic::Ordering::Release);
            tracing::warn!(
                "this vera.rs provider serves several nodes: a switch of its deployment's \
                 active device that voids a change it confirmed fails closed"
            );
        }
    }

    fn set_recovery_source(&self, source: std::sync::Weak<dyn acp::RecoverySource>) {
        if self.follower.recovery.set(source).is_err() {
            tracing::warn!("this vera.rs provider already has a recovery source");
        }
    }

    fn delegation_audience(&self) -> Option<acp::DelegationAudience> {
        Some(acp::DelegationAudience {
            worker: self.worker_did.clone(),
            deployment: self.deployment,
            genesis_id: self.genesis_id.get().copied(),
        })
    }

    fn caller_of_delegation(
        &self,
        token: &str,
    ) -> Option<Result<acp::AuthenticatedCaller, ProviderError>> {
        bearer::is_delegation(token).then(|| {
            bearer::caller_of_delegation(token, &self.worker_did, self.deployment)
                .map_err(|error| refused("caller delegation", error))
        })
    }

    fn caller_of_request_delegation(
        &self,
        token: &str,
        command: &crate::PolicyCommand<'_>,
    ) -> Option<Result<String, ProviderError>> {
        let audience = self.delegation_audience()?;
        bearer::caller_of_request_delegation(token, &audience, || request_digest(command))
            .map(|caller| caller.map_err(|error| refused("request delegation", error)))
    }
}

impl VeraRsProvider {
    /// Whether `term` is still the certified one. A pass asks before and
    /// after each proof it reads, so a page holds only what its term showed.
    fn still_recovering(&self, term: u64) -> Result<(), String> {
        if self.follower.certified() == term {
            return Ok(());
        }
        Err(format!("Vera recovery term changed from {term}"))
    }
}

#[async_trait]
impl acp::RecoveryVerifier for VeraRsProvider {
    async fn term(&self) -> Result<u64, String> {
        Ok(self.follower.certified())
    }

    async fn object_owner(
        &self,
        term: u64,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<Option<String>, String> {
        self.still_recovering(term)?;
        let policy = Self::policy_id_to_bytes32(policy_id)
            .map_err(|error| error.to_string())
            .map(hex::encode)?;
        let object = acp_light_client::Object {
            resource: resource.into(),
            id: object_id.into(),
        };
        let owner = verified(self.light_client.header_chain(), || {
            self.light_client.read_object_owner(&policy, &object)
        })
        .await
        .map_err(|error| {
            reach::read_error(&self.light_client, "recovery owner proof", error).to_string()
        })?;
        self.still_recovering(term)?;
        Ok(owner.map(|owner| did_spelling::defra(owner.0.as_str()).into_owned()))
    }

    async fn relationship(
        &self,
        term: u64,
        owner: &identity::Did,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &acp::OwedSubject,
    ) -> Result<acp::Observed, String> {
        self.still_recovering(term)?;
        // The deployment is asked about one explicit actor, as every other
        // observation is; any other subject cannot be read.
        let actor = match acp::Subject::from(subject.clone()) {
            acp::Subject::Entity(did) if !did.is_wildcard() => did.as_str().to_owned(),
            other => {
                return Ok(acp::Observed::Unobservable(format!(
                    "a recovery pass cannot read the subject {other:?}"
                )))
            }
        };
        // The deployment shows a relationship to the object's owner and to
        // the relation's managers. The pass reads the node's own record of
        // the object, whose owner the deployment was just seen to hold.
        let query = ActorRelationshipQuery {
            requester: owner.as_str(),
            actor: &actor,
            policy: policy_id,
            resource,
            object: object_id,
            relation,
        };
        let observed = verified(self.light_client.header_chain(), || async {
            exact_relationship::query(&self.light_client, query)
                .await
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
        })
        .await;
        self.still_recovering(term)?;
        match observed {
            Ok(true) => Ok(acp::Observed::Held),
            Ok(false) => Ok(acp::Observed::Absent),
            Err(error) => match error.downcast::<ProviderError>() {
                Ok(error) if matches!(*error, ProviderError::RelationshipObservationDenied) => {
                    Ok(acp::Observed::Unobservable(error.to_string()))
                }
                Ok(error) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            },
        }
    }
}

/// A delegation refused as `what`: one that does not hold at the node's
/// time keeps its window, so its signer can issue it again at that time.
fn refused(what: &str, error: bearer::CallerDelegationError) -> ProviderError {
    match error {
        bearer::CallerDelegationError::Window(window) => {
            ProviderError::DelegationOutsideWindow(window)
        }
        other => ProviderError::Config(format!("{what}: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::PrivateKey;
    use identity::Identity;
    use vera_client::Secp256k1SigningKey as SigningKey;

    #[nuclei::test]
    async fn trusted_consensus_key_is_required_before_connecting() {
        let root = tempfile::tempdir().unwrap();
        let keys = keyring::FileKeyring::open(root.path().join("keys"), b"test").unwrap();
        for (index, key) in ["", "not-hex", "00"].into_iter().enumerate() {
            let worker =
                NativeWorker::open(&root.path().join(index.to_string()), &keys, 9001).unwrap();
            let error = VeraRsProvider::new(
                "http://127.0.0.1:1".into(),
                key,
                GenesisSource::Unnamed,
                &[],
                worker,
                &AcpTuning::default(),
                None,
                &edge::testing::edge(),
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
        )
        .expect("the store takes the identity");
    }

    #[nuclei::test]
    async fn resolve_registered_or_passthrough_bearer_token_uses_request_token_for_remote_identity()
    {
        let did = "did:key:zRemoteVeraRsToken";
        let token = "device.jwt.token".to_string();
        store_remote_secp256r1_identity(did);
        let request = defra_core::RequestContext::default()
            .with_bearer(Some(did.to_string()), Some(token.clone()));

        let resolved = resolve_registered_or_passthrough_bearer_token_as(
            &request,
            did,
            "did:key:submitter",
            9001,
        )
        .await
        .expect("resolution should succeed")
        .expect("token should resolve");
        assert_eq!(resolved, token);
    }

    #[nuclei::test]
    async fn a_request_token_under_either_issuer_spelling_is_passed_through() {
        use base64::Engine;
        let key = SigningKey::from_slice(&[42; 32]).expect("valid key");
        let did = bearer::did_from_signing_key(&key, false);
        let token = |issuer: &str| {
            let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::json!({ "iss": issuer }).to_string());
            format!("header.{payload}.signature")
        };
        store_remote_secp256r1_identity(&did);

        for compressed in [true, false] {
            let carried = token(&bearer::did_from_signing_key(&key, compressed));
            let request = defra_core::RequestContext::default()
                .with_bearer(Some(did.clone()), Some(carried.clone()));
            let passed = resolve_registered_or_passthrough_bearer_token_as(
                &request,
                &did,
                "did:key:w",
                9001,
            )
            .await
            .expect("resolution should succeed");
            assert_eq!(passed, Some(carried));
        }
    }

    #[nuclei::test]
    async fn resolve_registered_or_passthrough_bearer_token_builds_local_secp256k1_token() {
        let private_key = crypto::generate_secp256k1().expect("should generate secp256k1 key");
        let raw_identity =
            identity::RawIdentity::from_secp256k1(private_key).expect("identity should build");
        let did = raw_identity.did().expect("did should derive").to_string();

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
        )
        .expect("the store takes the identity");

        let resolved =
            resolve_registered_or_passthrough_bearer_token(&did, "did:key:submitter", 9001)
                .await
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
    }
}

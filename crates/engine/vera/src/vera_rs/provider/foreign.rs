//! A provider for policies another deployment owns: the owner's home
//! (`proofs/tla/ForeignPolicy.tla`).
//!
//! It holds a light client of that deployment and nothing else: no worker,
//! no actor and no key. Every answer is a proof verified under the
//! deployment's consensus key and its genesis identity, which the node is
//! given with the policy and never takes from the home itself. It reads and
//! never writes: what changes the policy's state is made on the owner's
//! home, so every command is refused here.
//!
//! The freshness contract: every answer is made from the newest revision of
//! the home the light client verified, at most its freshness bound old
//! (`FreshnessPolicy::default`, 30 seconds), and a decision is reused while
//! that revision is the newest ([`VeraProvider::decision_state`]). A grant
//! or revocation on the home is seen once a revision holding it is
//! verified, so for up to the bound an answer may be one the home has
//! since changed. A home that cannot be reached answers from its newest
//! verified revision until that revision is older than the bound, and from
//! then on is [`ProviderError::Unavailable`] (`reach`), never a denial; so
//! is a home not yet proven to serve the given genesis. Each new state root
//! of the home is reported to the registry that built the provider, which
//! re-drives what waits on the policies that home owns.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use acp_light_client::{AcpLightClient, AcpTransport, FreshnessPolicy};
use alloy_primitives::B256;
use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;
use vera_domain::ConsensusPublicKey;
use vera_modules::acp::types::PolicyRecord;

use super::{access_request, exact_relationship, format_root, genesis, reach, verified};
use crate::provider::{
    AcpLightClientStatus, ActorRelationshipQuery, ProviderError, ProviderPolicyInfo, SubjectRef,
    VeraProvider,
};
use crate::vera_rs::did_spelling;

/// Told each time the home's verified state root changes.
pub trait HomeStateListener: MaybeSendSync {
    fn state_changed(&self);
}

/// Whether the home serves the deployment the node was given: not yet read,
/// the given one, or another.
const UNCHECKED: u8 = 0;
const MATCHES: u8 = 1;
const DIFFERS: u8 = 2;

/// A read-only provider of a deployment other than the node's own.
pub struct ForeignProvider {
    light_client: Arc<AcpLightClient>,
    transport: Arc<dyn AcpTransport>,
    trusted: ConsensusPublicKey,
    genesis_id: [u8; 32],
    served: Arc<AtomicU8>,
    _observer: edge::Service,
}

impl ForeignProvider {
    /// A provider of the deployment whose consensus key is
    /// `trusted_consensus_key` and whose genesis block is `genesis_id`,
    /// reached through `acp`. Nothing is asked of the home here: one that is
    /// away when it is added is read once it answers. Its light client and
    /// observer run on `edge`, and `listener` hears of every new state root.
    pub async fn connect(
        acp: Arc<dyn AcpTransport>,
        trusted_consensus_key: &str,
        genesis_id: [u8; 32],
        listener: Arc<dyn HomeStateListener>,
        edge: &edge::Edge,
    ) -> Result<Self, ProviderError> {
        let trusted = super::trusted_key(trusted_consensus_key)?;
        let key = trusted_consensus_key.to_owned();
        let transport = Arc::clone(&acp);
        let light_client = edge::compat(async move {
            AcpLightClient::for_deployment(
                acp,
                &key,
                B256::from(genesis_id),
                10,
                FreshnessPolicy::default(),
            )
        })
        .await
        .map(Arc::new)
        .map_err(|e| ProviderError::Config(format!("foreign light client: {e}")))?;
        let served = Arc::new(AtomicU8::new(UNCHECKED));
        let _observer = edge.spawn_service(edge::compat(observe(
            light_client.clone(),
            trusted.clone(),
            genesis_id,
            served.clone(),
            listener,
        )));
        Ok(Self {
            light_client,
            transport,
            trusted,
            genesis_id,
            served,
            _observer,
        })
    }

    /// Refuses every read until the home was shown to serve the deployment
    /// the node was given: retryably while that is not yet proven, and for
    /// good once it was shown to serve another. A read is also refused,
    /// retryably and at once, while the newest revision the light client
    /// verified is older than its freshness bound: the home has not been
    /// heard from for that long, and waiting on it inside a query would
    /// spend the query's own deadline (`proofs/tla/ForeignPolicy.tla`
    /// HomeDecision).
    async fn serves_given(&self) -> Result<(), ProviderError> {
        let mut served = self.served.load(Ordering::Acquire);
        if served == DIFFERS {
            return serving(DIFFERS, self.genesis_id);
        }
        let state = self
            .light_client
            .header_chain()
            .fresh_state()
            .map_err(|e| ProviderError::Unavailable(format!("the foreign home is away: {e}")))?;
        if served == UNCHECKED {
            match genesis::read(self.light_client.proof_client(), &self.trusted).await {
                Ok(id) if id == self.genesis_id => {
                    served = MATCHES;
                    self.served.store(served, Ordering::Release);
                }
                Ok(id) => {
                    tracing::warn!(
                        served = %hex::encode(id),
                        given = %hex::encode(self.genesis_id),
                        "a foreign home serves another deployment than the one its policy names"
                    );
                    self.served.store(DIFFERS, Ordering::Release);
                    return serving(DIFFERS, self.genesis_id);
                }
                Err(error) => {
                    return Err(ProviderError::Unavailable(format!(
                        "the foreign home's genesis identity is not yet proven: {error}"
                    )))
                }
            }
        }
        match genesis::descends_from(
            self.light_client.proof_client(),
            self.transport.as_ref(),
            &self.trusted,
            self.genesis_id,
            &state,
        )
        .await
        {
            Ok(true) => {
                if served != MATCHES {
                    self.served.store(MATCHES, Ordering::Release);
                }
                Ok(())
            }
            Ok(false) => {
                self.served.store(DIFFERS, Ordering::Release);
                Err(ProviderError::Config(format!(
                    "the foreign home changed to a history that does not descend from 0x{}",
                    hex::encode(self.genesis_id)
                )))
            }
            Err(_) => Err(ProviderError::Unavailable(format!(
                "the foreign home did not prove that its revision descends from 0x{}",
                hex::encode(self.genesis_id)
            ))),
        }
    }

    /// The error of an observation read as `what`: a query error the home
    /// gave while the light client holds no fresh state did not reach it.
    fn unreached(
        &self,
        what: &str,
        error: Box<dyn std::error::Error + Send + Sync>,
    ) -> ProviderError {
        match error.downcast::<ProviderError>() {
            Ok(error) => match *error {
                ProviderError::Query(message)
                    if self.light_client.header_chain().fresh_state().is_err() =>
                {
                    ProviderError::Unavailable(format!("{what}: {message}"))
                }
                error => error,
            },
            Err(error) => reach::read_error(&self.light_client, what, error),
        }
    }

    fn policy_key(policy_id: &str) -> Result<String, ProviderError> {
        vera_client::parse_policy_id(policy_id)
            .map(hex::encode)
            .map_err(|e| ProviderError::Query(e.to_string()))
    }
}

/// Whether a home whose genesis check stands at `served` may be read: only
/// once it was proven to serve `genesis_id`. Until then nothing it says is
/// known to be that deployment's, so a read is retryably unavailable.
fn serving(served: u8, genesis_id: [u8; 32]) -> Result<(), ProviderError> {
    match served {
        MATCHES => Ok(()),
        DIFFERS => Err(ProviderError::Config(format!(
            "the foreign home does not serve the deployment whose genesis is 0x{}",
            hex::encode(genesis_id)
        ))),
        _ => Err(ProviderError::Unavailable(format!(
            "the foreign home is not yet proven to serve the deployment whose genesis is 0x{}",
            hex::encode(genesis_id)
        ))),
    }
}

/// A command refused: what changes a foreign policy's state is made on the
/// home that owns it.
fn owned_elsewhere(what: &str) -> ProviderError {
    ProviderError::Unsupported(format!(
        "{what} under a policy another deployment owns is made on that deployment's home"
    ))
}

/// Follows the home's verified heads: proves its genesis identity once, and
/// tells `listener` of every state root that differs from the one before.
async fn observe(
    light_client: Arc<AcpLightClient>,
    trusted: ConsensusPublicKey,
    genesis_id: [u8; 32],
    served: Arc<AtomicU8>,
    listener: Arc<dyn HomeStateListener>,
) {
    let mut next_height = 1u64;
    let mut previous_root = None;
    loop {
        let sync = match light_client
            .wait_for_height(next_height, super::NEXT_HEIGHT)
            .await
        {
            Ok(sync) => sync,
            Err(_) => {
                next_height = light_client
                    .header_chain()
                    .latest_height()
                    .saturating_add(1);
                continue;
            }
        };
        if served.load(Ordering::Acquire) == UNCHECKED {
            match genesis::read(light_client.proof_client(), &trusted).await {
                Ok(id) if id == genesis_id => served.store(MATCHES, Ordering::Release),
                Ok(id) => {
                    tracing::warn!(
                        served = %hex::encode(id),
                        given = %hex::encode(genesis_id),
                        "a foreign home serves another deployment than the one its policy names"
                    );
                    served.store(DIFFERS, Ordering::Release);
                }
                Err(error) => tracing::debug!(%error, "foreign genesis identity not yet proven"),
            }
        }
        if previous_root != Some(sync.module_state_root) {
            light_client
                .cache()
                .invalidate_stale(sync.module_state_root);
            listener.state_changed();
        }
        previous_root = Some(sync.module_state_root);
        next_height = sync.height.saturating_add(1);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl VeraProvider for ForeignProvider {
    /// No worker of this node submits to the home.
    fn authorized_account(&self) -> String {
        String::new()
    }

    /// A delete archives nothing on the home: the owner's home does.
    fn archive_object_on_document_delete(&self) -> bool {
        false
    }

    async fn create_bearer_token(&self, _did: &str) -> Result<String, ProviderError> {
        Err(owned_elsewhere("a bearer token"))
    }

    fn self_did(&self) -> Option<String> {
        None
    }

    fn canonical_actor(&self, did: &str) -> String {
        did_spelling::defra(did).into_owned()
    }

    async fn create_policy(
        &self,
        _policy_yaml: &str,
        _nonce: Option<acp::PolicyNonce>,
    ) -> Result<String, ProviderError> {
        Err(owned_elsewhere("a policy"))
    }

    async fn register_object(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relationships: &[acp::DerivedRelationship],
    ) -> Result<(), ProviderError> {
        Err(owned_elsewhere("a registration"))
    }

    async fn archive_object(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> Result<(), ProviderError> {
        Err(owned_elsewhere("an archive"))
    }

    async fn set_relationship(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _subject: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        Err(owned_elsewhere("a relationship"))
    }

    async fn delete_relationship(
        &self,
        _bearer_token: &str,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _relation: &str,
        _subject: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        Err(owned_elsewhere("a relationship"))
    }

    async fn query_policy(
        &self,
        policy_id: &str,
    ) -> Result<Option<ProviderPolicyInfo>, ProviderError> {
        self.serves_given().await?;
        let id = Self::policy_key(policy_id)?;
        let proof = verified(self.light_client.header_chain(), || {
            self.light_client.read_policy(&id)
        })
        .await
        .map_err(|e| reach::read_error(&self.light_client, "foreign policy proof", e))?;
        self.serves_given().await?;
        let Some(bytes) = proof.value.as_deref() else {
            return Ok(None);
        };
        let record: PolicyRecord = serde_json::from_slice(bytes)
            .map_err(|e| ProviderError::Query(format!("policy JSON: {e}")))?;
        if record.policy.id.as_str() != id {
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
        self.serves_given().await?;
        let policy = Self::policy_key(policy_id)?;
        let object = acp_light_client::Object {
            resource: resource.into(),
            id: object_id.into(),
        };
        let owner = verified(self.light_client.header_chain(), || {
            self.light_client.read_object_owner(&policy, &object)
        })
        .await
        .map_err(|e| reach::read_error(&self.light_client, "foreign owner proof", e))?;
        self.serves_given().await?;
        Ok(match owner {
            Some(owner) => (true, did_spelling::defra(owner.0.as_str()).into_owned()),
            None => (false, String::new()),
        })
    }

    async fn query_actor_relationship(
        &self,
        query: ActorRelationshipQuery<'_>,
    ) -> Result<bool, ProviderError> {
        self.serves_given().await?;
        let result = verified(self.light_client.header_chain(), || async {
            exact_relationship::query(&self.light_client, query)
                .await
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
        })
        .await
        .map_err(|error| self.unreached("foreign relationship", error));
        self.serves_given().await?;
        result
    }

    async fn verify_transport_sentinel(
        &self,
        query: &acp::TransportSentinelQuery,
    ) -> Result<bool, ProviderError> {
        self.serves_given().await?;
        let result = verified(self.light_client.header_chain(), || async {
            exact_relationship::sentinel(&self.light_client, query)
                .await
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
        })
        .await
        .map_err(|error| self.unreached("foreign sentinel", error));
        self.serves_given().await?;
        result
    }

    async fn verify_access(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        permission: &str,
        actor_did: &str,
    ) -> Result<bool, ProviderError> {
        self.serves_given().await?;
        let request = access_request(resource, object_id, permission, actor_did)?;
        let result = verified(self.light_client.header_chain(), || {
            self.light_client.verify_access(policy_id, &request)
        })
        .await
        .map_err(|e| reach::read_error(&self.light_client, "foreign verified permission", e));
        self.serves_given().await?;
        result
    }

    /// Every decision is a permission proof of the home verified at a
    /// revision no older than the light client's freshness limit.
    fn proves_decisions(&self) -> bool {
        true
    }

    /// The home's verified state while it is fresh: a decision is remembered
    /// under it, and under nothing once the home has gone quiet for longer
    /// than the freshness limit.
    fn decision_state(&self) -> Option<String> {
        self.light_client
            .header_chain()
            .fresh_state()
            .ok()
            .map(|state| format_root(state.module_state_root))
    }

    fn acp_light_client_status(&self) -> Result<AcpLightClientStatus, ProviderError> {
        let sync = self.light_client.header_chain().state();
        Ok(AcpLightClientStatus {
            height: sync.as_ref().map_or(0, |state| state.height),
            chain_time: sync.as_ref().map(|state| state.timestamp),
            module_state_root: sync
                .as_ref()
                .map_or_else(String::new, |state| format_root(state.module_state_root)),
            cache_entries: self.light_client.cache().len(),
            last_invalidation_height: 0,
            connected: sync.is_some(),
        })
    }
}

#[cfg(test)]
#[path = "foreign_tests.rs"]
mod tests;

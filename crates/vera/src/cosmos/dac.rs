//! Vera DocumentACP implementation.
//!
//! Implements the `DocumentACP` trait by delegating to a `VeraProvider`
//! for all on-chain communication. The provider abstraction allows swapping
//! between Cosmos SDK and EVM backends.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSend;
use identity::Did;

use acp::{DocumentACP, DocumentPermission, Identity, Result, Subject};

use crate::access_cache::AccessCache;
use crate::decisions::{DecisionKey, RemoteDecisions};
use crate::provider::{AcpLightClientStatus, ProviderError, SubjectRef, VeraProvider};

/// DocumentACP backed by Vera's on-chain x/acp module.
///
/// Delegates write and read operations to a `VeraProvider` implementation.
/// By default it caches `verify_access` results to avoid redundant network
/// roundtrips. vera.rs can opt out because remote ACP state may change outside
/// DefraDB's mutation path.
pub struct VeraDocumentACP {
    provider: Arc<dyn VeraProvider>,
    access_cache: Option<Arc<AccessCache>>,
    /// Remote decisions: one call per tuple in flight, a deadline, retries.
    decisions: inputs::Service<RemoteDecisions>,
    #[cfg(not(target_arch = "wasm32"))]
    event_subscriber: Option<super::event_subscriber::CosmosEventSubscriber>,
    /// The request this ACP serves, for one bound to it: the bearer tokens
    /// its callers write with.
    context: Option<defra_core::RequestContext>,
}

/// How long a remote decision may take when the caller sets nothing else.
const DECISION_TIMEOUT: Duration = Duration::from_secs(5);

impl VeraDocumentACP {
    pub fn new(provider: Arc<dyn VeraProvider>, cache_ttl: Duration) -> Self {
        Self::with_cache(provider, Some(Arc::new(AccessCache::new(cache_ttl))))
    }

    pub fn without_access_cache(provider: Arc<dyn VeraProvider>) -> Self {
        Self::with_cache(provider, None)
    }

    fn with_cache(provider: Arc<dyn VeraProvider>, access_cache: Option<Arc<AccessCache>>) -> Self {
        Self {
            decisions: Self::decisions(&provider, inputs::Runtime::on_demand(), DECISION_TIMEOUT),
            provider,
            access_cache,
            #[cfg(not(target_arch = "wasm32"))]
            event_subscriber: None,
            context: None,
        }
    }

    fn decisions(
        provider: &Arc<dyn VeraProvider>,
        runtime: inputs::Runtime,
        request_timeout: Duration,
    ) -> inputs::Service<RemoteDecisions> {
        inputs::Service::new(
            RemoteDecisions {
                provider: Arc::clone(provider),
            },
            crate::decisions::config(request_timeout),
            runtime,
        )
    }

    /// A cached ACP as `tuning` sets it, its remote decisions on the network
    /// runtime `network` when the node has one, else on the process's
    /// on-demand runtime.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn tuned(
        provider: Arc<dyn VeraProvider>,
        tuning: &crate::AcpTuning,
        network: Option<&edge::Edge>,
    ) -> Self {
        let runtime = network.map_or_else(inputs::Runtime::on_demand, |edge| {
            inputs::Runtime::edge(edge.clone())
        });
        Self::new(provider, tuning.cache_ttl).on_runtime(runtime, tuning.request_timeout)
    }

    /// Remote decisions run on the network runtime `edge`, each within twice
    /// `request_timeout` (one retry).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn on_network(self, edge: &edge::Edge, request_timeout: Duration) -> Self {
        self.on_runtime(inputs::Runtime::edge(edge.clone()), request_timeout)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn on_runtime(mut self, runtime: inputs::Runtime, request_timeout: Duration) -> Self {
        self.decisions = Self::decisions(&self.provider, runtime, request_timeout);
        self
    }

    /// The metrics of remote decisions.
    pub fn decision_stats(&self) -> &inputs::Stats {
        self.decisions.stats()
    }

    /// Invalidate the access cache from the Cosmos chain's ACP events, read
    /// over a WebSocket that runs on the network runtime `edge`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_cosmos_event_invalidation(
        mut self,
        websocket_url: impl Into<String>,
        edge: &edge::Edge,
    ) -> std::result::Result<Self, ProviderError> {
        let cache = self.access_cache.as_ref().ok_or_else(|| {
            ProviderError::Config(
                "Cosmos event invalidation requires the access cache to be enabled".into(),
            )
        })?;
        self.event_subscriber = Some(super::event_subscriber::CosmosEventSubscriber::start(
            websocket_url.into(),
            Arc::clone(cache),
            edge,
        )?);
        // Events now reach the cache: denials and registrations are cached
        // too, cut short by the events like allows (D36).
        cache.receive_events();
        Ok(self)
    }

    fn invalidate_cached_access(&self, policy_id: &str) {
        if let Some(cache) = &self.access_cache {
            // Other objects can inherit access through this object's relationships.
            cache.invalidate_policy(policy_id);
        }
    }

    /// Create a policy on Vera. Returns the policy ID.
    pub async fn add_policy(
        &self,
        _creator_did: &str,
        policy_yaml: &str,
    ) -> std::result::Result<String, ProviderError> {
        let yaml = policy_yaml.to_owned();
        self.remote(move |provider| async move { provider.create_policy(&yaml).await })
            .await
    }

    pub async fn get_policy(
        &self,
        policy_id: &str,
    ) -> std::result::Result<Option<acp::Policy>, ProviderError> {
        let id = policy_id.to_owned();
        let Some(info) = self
            .remote(move |provider| async move { provider.query_policy(&id).await })
            .await?
        else {
            return Ok(None);
        };

        let Some(raw_policy) = info.raw_policy else {
            return Err(ProviderError::Query(format!(
                "policy '{}' is not available from the configured provider",
                policy_id
            )));
        };

        acp::policy_yaml::check_duplicate_yaml_keys(&raw_policy)
            .map_err(|e| ProviderError::Query(format!("policy parse: {}", e)))?;
        let parsed = acp::policy_yaml::parse_policy_yaml(&raw_policy)
            .map_err(|e| ProviderError::Query(format!("policy parse: {}", e)))?;
        let mut policy = acp::policy_yaml::build_policy(&parsed, 1)
            .map_err(|e| ProviderError::Query(format!("policy parse: {}", e)))?;
        policy.id = info.id;
        Ok(Some(policy))
    }

    /// Whether the document is registered: from the cache while events
    /// reach it, else asked of the provider.
    async fn is_registered(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> std::result::Result<bool, acp::Error> {
        if let Some(registered) = self
            .access_cache
            .as_ref()
            .and_then(|cache| cache.registration(policy_id, resource_name, doc_id))
        {
            return Ok(registered);
        }
        let generation = self
            .access_cache
            .as_ref()
            .map(|cache| cache.registration_generation());
        let (is_registered, _owner) = self.object_owner(policy_id, resource_name, doc_id).await?;
        if let (Some(cache), Some(generation)) = (&self.access_cache, generation) {
            cache.remember_registration(
                policy_id,
                resource_name,
                doc_id,
                is_registered,
                generation,
            );
        }
        Ok(is_registered)
    }

    async fn create_bearer_token(&self, did: &str) -> std::result::Result<String, acp::Error> {
        let context =
            defra_core::RequestContext::bound_or_ambient(self.context.as_ref()).into_owned();
        let did = did.to_owned();
        self.remote(
            move |provider| async move { provider.create_bearer_token_as(&context, &did).await },
        )
        .await
        .map_err(provider_err)
    }

    async fn object_owner(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> std::result::Result<(bool, String), acp::Error> {
        let [policy, resource, doc] = owned([policy_id, resource_name, doc_id]);
        self.remote(move |provider| async move {
            provider.query_object_owner(&policy, &resource, &doc).await
        })
        .await
        .map_err(provider_err)
    }

    /// The provider round trip `call` makes, run where round trips run: a
    /// call on a pool worker crosses to the network, whose reactor the
    /// provider's client needs ([`inputs::Runtime::hand_off`]).
    async fn remote<T, F, Fut>(&self, call: F) -> std::result::Result<T, ProviderError>
    where
        F: FnOnce(Arc<dyn VeraProvider>) -> Fut + MaybeSend + 'static,
        Fut: Future<Output = std::result::Result<T, ProviderError>> + MaybeSend + 'static,
        T: MaybeSend + 'static,
    {
        let provider = Arc::clone(&self.provider);
        self.decisions
            .runtime()
            .hand_off(move || call(provider))
            .await
            .unwrap_or_else(|| {
                Err(ProviderError::Unavailable(
                    "the request did not complete; try again".into(),
                ))
            })
    }

    pub fn acp_light_client_status(
        &self,
    ) -> std::result::Result<AcpLightClientStatus, ProviderError> {
        self.provider.acp_light_client_status()
    }
}

fn did_to_subject(target: &Did) -> SubjectRef {
    if target.as_str() == "*" {
        SubjectRef::AllActors
    } else {
        SubjectRef::Actor(target.as_str().to_string())
    }
}

/// Lowers an actor [`Subject`] (`Entity`/`Wildcard`) to the bare-DID actor the
/// existing relationship path operates on. Non-actor subjects must be routed to
/// the structured `*_relationship_subject` provider methods instead.
fn subject_to_actor_did(target: &Subject) -> Did {
    match target {
        Subject::Wildcard => Did::wildcard(),
        Subject::Entity(did) => Did::new_unchecked(did.to_string()),
        _ => Did::new_unchecked(String::new()),
    }
}

/// Borrowed arguments made owned, for a round trip that may cross to the
/// network.
fn owned<const N: usize>(parts: [&str; N]) -> [String; N] {
    parts.map(str::to_owned)
}

fn provider_err(e: impl std::fmt::Display) -> acp::Error {
    acp::Error::Storage(format!("Vera: {}", e))
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl acp::RemotePolicies for VeraDocumentACP {
    async fn add_policy(&self, creator: &str, policy: &str) -> std::result::Result<String, String> {
        VeraDocumentACP::add_policy(self, creator, policy)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_policy(
        &self,
        policy_id: &str,
    ) -> std::result::Result<Option<acp::Policy>, String> {
        VeraDocumentACP::get_policy(self, policy_id)
            .await
            .map_err(|e| e.to_string())
    }

    fn light_client_status(&self) -> std::result::Result<acp::LightClientStatus, String> {
        let status = self.acp_light_client_status().map_err(|e| e.to_string())?;
        Ok(acp::LightClientStatus {
            height: status.height,
            module_state_root: status.module_state_root,
            cache_entries: status.cache_entries,
            last_invalidation_height: status.last_invalidation_height,
            connected: status.connected,
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentACP for VeraDocumentACP {
    fn bind_request(&self, context: &defra_core::RequestContext) -> Option<Arc<dyn DocumentACP>> {
        Some(Arc::new(Self {
            provider: Arc::clone(&self.provider),
            access_cache: self.access_cache.clone(),
            decisions: self.decisions.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            event_subscriber: None,
            context: Some(context.clone()),
        }))
    }

    fn bearer_token_for(&self, did: &str) -> Option<String> {
        defra_core::RequestContext::bound_or_ambient(self.context.as_ref()).bearer_token_for(did)
    }

    fn unregistered_documents_are_public(&self) -> bool {
        self.provider.unregistered_documents_are_public()
    }

    async fn register_doc_object(
        &self,
        identity: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        let bearer_token = self.create_bearer_token(identity.as_str()).await?;
        let [policy, resource, doc] = owned([policy_id, resource_name, doc_id]);
        self.remote(move |provider| async move {
            provider
                .register_object(&bearer_token, &policy, &resource, &doc)
                .await
        })
        .await
        .map_err(provider_err)?;
        self.invalidate_cached_access(policy_id);
        Ok(())
    }

    async fn is_doc_registered(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        let (is_registered, _owner) = self.object_owner(policy_id, resource_name, doc_id).await?;
        Ok(is_registered)
    }

    async fn get_doc_owner(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<Option<Did>> {
        let (is_registered, owner) = self.object_owner(policy_id, resource_name, doc_id).await?;

        if !is_registered {
            return Ok(None);
        }

        Did::new(&owner)
            .map(Some)
            .map_err(|error| acp::Error::Storage(format!("invalid owner DID: {error}")))
    }

    async fn check_doc_access(
        &self,
        identity: &Identity,
        permission: DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        if self.provider.unregistered_documents_are_public()
            && !self.is_registered(policy_id, resource_name, doc_id).await?
        {
            return Ok(true);
        }

        let actor_did = match identity.did() {
            Some(did) => did.as_str().to_string(),
            None => "did:key:anonymous".to_string(),
        };

        if let Some(cache) = &self.access_cache {
            if let Some(cached) = cache.get(
                &actor_did,
                policy_id,
                resource_name,
                doc_id,
                permission.as_str(),
            ) {
                return Ok(cached);
            }
        }

        let pending = self.access_cache.as_ref().map(|cache| {
            cache.begin_check(
                &actor_did,
                policy_id,
                resource_name,
                doc_id,
                permission.as_str(),
            )
        });
        let result = self
            .decisions
            .get(DecisionKey {
                actor: actor_did,
                policy: policy_id.to_string(),
                resource: resource_name.to_string(),
                object: doc_id.to_string(),
                permission: permission.as_str().to_string(),
                state: self.provider.decision_state(),
            })
            .await
            .map_err(|error| match error {
                inputs::Error::Source(error) => provider_err(error),
                other => provider_err(other),
            })?;

        // A denial is cached only where invalidation events reach the cache
        // (D36); an allow always is.
        if let Some(pending) = pending {
            if result
                || self
                    .access_cache
                    .as_ref()
                    .is_some_and(|cache| cache.caches_denials())
            {
                pending.complete(result);
            }
        }

        Ok(result)
    }

    async fn create_access_decision(
        &self,
        identity: &Identity,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
        permission: &str,
    ) -> Result<Option<String>> {
        let Some(actor_did) = identity.did() else {
            return Ok(None);
        };

        let [policy, resource, object, permission, actor] = owned([
            policy_id,
            resource_name,
            object_id,
            permission,
            actor_did.as_str(),
        ]);
        self.remote(move |provider| async move {
            provider
                .create_access_decision(&policy, &resource, &object, &permission, &actor)
                .await
        })
        .await
        .map_err(provider_err)
    }

    async fn add_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        relation: &str,
        _managing_relations: &[String],
    ) -> Result<bool> {
        let bearer_token = self.create_bearer_token(requestor.as_str()).await?;
        let subject = did_to_subject(target);
        let [policy, resource, doc, relation] = owned([policy_id, resource_name, doc_id, relation]);
        let result = self
            .remote(move |provider| async move {
                provider
                    .set_relationship(&bearer_token, &policy, &resource, &doc, &relation, &subject)
                    .await
            })
            .await
            .map_err(provider_err)?;

        self.invalidate_cached_access(policy_id);

        Ok(result)
    }

    async fn delete_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        relation: &str,
        _managing_relations: &[String],
    ) -> Result<bool> {
        let bearer_token = self.create_bearer_token(requestor.as_str()).await?;
        let subject = did_to_subject(target);
        let [policy, resource, doc, relation] = owned([policy_id, resource_name, doc_id, relation]);
        let result = self
            .remote(move |provider| async move {
                provider
                    .delete_relationship(
                        &bearer_token,
                        &policy,
                        &resource,
                        &doc,
                        &relation,
                        &subject,
                    )
                    .await
            })
            .await
            .map_err(provider_err)?;

        self.invalidate_cached_access(policy_id);

        Ok(result)
    }

    async fn add_relationship(
        &self,
        requestor: &Did,
        target: Subject,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> Result<bool> {
        match target {
            Subject::Entity(_) | Subject::Wildcard => {
                let actor = subject_to_actor_did(&target);
                self.add_actor_relationship(
                    requestor,
                    &actor,
                    policy_id,
                    resource_name,
                    doc_id,
                    relation,
                    managing_relations,
                )
                .await
            }
            Subject::EntitySet { .. } => {
                let (kind, sr, so, srel) =
                    zanzibar::encode_subject(&target).map_err(acp::Error::from)?;
                let [policy, resource, doc, relation] =
                    owned([policy_id, resource_name, doc_id, relation]);
                let result = self
                    .remote(move |provider| async move {
                        provider
                            .set_relationship_subject(
                                &policy, &resource, &doc, &relation, kind, &sr, &so, &srel,
                            )
                            .await
                    })
                    .await
                    .map_err(provider_err)?;
                self.invalidate_cached_access(policy_id);
                Ok(result)
            }
            other => Err(acp::Error::UnsupportedSubject(other.to_string())),
        }
    }

    async fn delete_relationship(
        &self,
        requestor: &Did,
        target: Subject,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> Result<bool> {
        match target {
            Subject::Entity(_) | Subject::Wildcard => {
                let actor = subject_to_actor_did(&target);
                self.delete_actor_relationship(
                    requestor,
                    &actor,
                    policy_id,
                    resource_name,
                    doc_id,
                    relation,
                    managing_relations,
                )
                .await
            }
            Subject::EntitySet { .. } => {
                let (kind, sr, so, srel) =
                    zanzibar::encode_subject(&target).map_err(acp::Error::from)?;
                let [policy, resource, doc, relation] =
                    owned([policy_id, resource_name, doc_id, relation]);
                let result = self
                    .remote(move |provider| async move {
                        provider
                            .delete_relationship_subject(
                                &policy, &resource, &doc, &relation, kind, &sr, &so, &srel,
                            )
                            .await
                    })
                    .await
                    .map_err(provider_err)?;
                self.invalidate_cached_access(policy_id);
                Ok(result)
            }
            other => Err(acp::Error::UnsupportedSubject(other.to_string())),
        }
    }

    async fn unregister_doc_object(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        // Shared native ACP objects outlive a local replica's tombstone. The
        // replicated delete can still be rejected elsewhere, so do not revoke
        // shared grants as a side effect of this local document operation.
        // Operators can still archive explicitly through the provider API.
        if !self.provider.archive_object_on_document_delete() {
            return Ok(());
        }

        let (_is_registered, owner_did) =
            self.object_owner(policy_id, resource_name, doc_id).await?;

        if owner_did.is_empty() {
            return Ok(());
        }

        // vera.rs uses node's DID for archive; Cosmos uses the owner's DID
        let archive_did = self
            .provider
            .self_did()
            .unwrap_or_else(|| owner_did.clone());
        let bearer_token = self.create_bearer_token(&archive_did).await?;
        let [policy, resource, doc] = owned([policy_id, resource_name, doc_id]);
        self.remote(move |provider| async move {
            provider
                .archive_object(&bearer_token, &policy, &resource, &doc)
                .await
        })
        .await
        .map_err(provider_err)?;

        self.invalidate_cached_access(policy_id);

        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[async_trait::async_trait]
impl acp::PolicyLookup for VeraDocumentACP {
    async fn get_policy(
        &self,
        policy_id: &str,
    ) -> std::result::Result<Option<acp::Policy>, String> {
        VeraDocumentACP::get_policy(self, policy_id)
            .await
            .map_err(|e| e.to_string())
    }
}

//! Vera DocumentACP implementation.
//!
//! Implements the `DocumentACP` trait by delegating to a `VeraProvider`
//! for all communication with Vera.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSend;
use identity::Did;

use acp::{DocumentACP, DocumentPermission, Identity, Result, Subject};

use crate::decisions::{DecisionKey, RemoteDecisions};
use crate::provider::{
    AcpLightClientStatus, ActorRelationshipQuery, ProviderError, SubjectRef, VeraProvider,
};
use crate::{PolicyAction, PolicyCommand, PolicySubject};

/// DocumentACP backed by Vera's ACP module.
///
/// Delegates write and read operations to a `VeraProvider` implementation.
/// It keeps no decision cache of its own: Vera's ACP state changes outside
/// DefraDB's mutation path, so only a decision read at a verified state is
/// reused, keyed by that state (see `decisions`).
pub struct VeraDocumentACP {
    provider: Arc<dyn VeraProvider>,
    /// Remote decisions: one call per tuple in flight, a deadline, retries.
    decisions: inputs::Service<RemoteDecisions>,
    /// The request this ACP serves, for one bound to it: the bearer tokens
    /// its callers write with.
    context: Option<defra_core::RequestContext>,
}

/// How long a remote decision may take when the caller sets nothing else.
const DECISION_TIMEOUT: Duration = Duration::from_secs(5);

impl VeraDocumentACP {
    pub fn new(provider: Arc<dyn VeraProvider>) -> Self {
        Self {
            decisions: Self::decisions(&provider, inputs::Runtime::on_demand(), DECISION_TIMEOUT),
            provider,
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

    /// The DID the provider itself acts as, for a provider that has one.
    pub fn self_did(&self) -> Option<String> {
        self.provider.self_did()
    }

    /// Create a policy on Vera, its ID derived from `nonce` where the
    /// provider derives one. Returns the policy ID.
    ///
    /// The provider creates as its own actor, which owns the policy. A
    /// nonce names the policy that actor gets, so one supplied by another
    /// caller is refused: the ID that caller computed from its own DID is
    /// not the one it would get. Without a nonce any permitted caller adds
    /// a policy the actor owns.
    pub async fn add_policy(
        &self,
        creator_did: &str,
        policy_yaml: &str,
        nonce: Option<acp::PolicyNonce>,
    ) -> std::result::Result<String, ProviderError> {
        if let (Some(_), Some(actor)) = (nonce, self.provider.self_did()) {
            if !crate::vera_rs::did_spelling::same_actor(creator_did, &actor) {
                return Err(ProviderError::Config(format!(
                    "a policy nonce names the policy the node's actor {actor} creates; \
                     {creator_did} is not that actor"
                )));
            }
        }
        let yaml = policy_yaml.to_owned();
        self.remote(move |provider| async move { provider.create_policy(&yaml, nonce).await })
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

    /// Whether the document is registered, as its provider answers.
    async fn is_registered(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> std::result::Result<bool, acp::Error> {
        let (is_registered, _owner) = self.object_owner(policy_id, resource_name, doc_id).await?;
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

    /// What lets the node send a command as `actor`: its own key for
    /// `actor`, or the delegation the request this ACP serves carries for
    /// it. One the request presents as bound to one request
    /// ([`RequestContext::with_request_delegation`](defra_core::RequestContext::with_request_delegation))
    /// counts for `command` when it names it, and for nothing when no
    /// command is given; presented as a plain delegation it counts for
    /// nothing at all.
    fn authority(
        &self,
        actor: &Did,
        command: Option<&crate::PolicyCommand<'_>>,
    ) -> Result<acp::OwedAuthority> {
        let canonical = self.provider.canonical_actor(actor.as_str());
        if self.provider.holds_key_for(actor.as_str()) || self.provider.holds_key_for(&canonical) {
            return Ok(acp::OwedAuthority::NodeKey);
        }
        let unavailable = |reason: String| acp::Error::OwedAuthorityUnavailable {
            actor: actor.to_string(),
            reason,
        };
        let token = self
            .bearer_token_for(actor.as_str())
            .or_else(|| self.bearer_token_for(&canonical))
            .ok_or_else(|| {
                unavailable(
                    "the node holds no key for it and the request carries no delegation".into(),
                )
            })?;
        // Presented as bound to one request, the token counts for the
        // command it names and is never taken as a plain delegation.
        if defra_core::RequestContext::bound_or_ambient(self.context.as_ref())
            .bearer_names_request()
        {
            let bound =
                command.and_then(|command| self.request_bound(actor.as_str(), &token, command));
            return match bound {
                Some(Ok(())) => Ok(acp::OwedAuthority::Delegation { token }),
                Some(Err(refusal)) => Err(refusal),
                None if command.is_some() => Err(unavailable(
                    "the delegation presented as bound to one request is bound to none".into(),
                )),
                None => Err(unavailable(
                    "a delegation bound to one request names no command of this write".into(),
                )),
            };
        }
        match self.provider.caller_of_delegation(&token) {
            Some(Ok(caller)) if self.provider.canonical_actor(caller.did.as_str()) == canonical => {
                Ok(acp::OwedAuthority::Delegation { token })
            }
            Some(Ok(caller)) => Err(unavailable(format!(
                "the request's delegation is signed by {}",
                caller.did
            ))),
            Some(Err(error)) => Err(request_bound::delegation_refused(actor.as_str(), error)),
            None if self.provider.delegation_audience().is_some() => Err(unavailable(
                "the request's token is not a delegation to this node".into(),
            )),
            // A verified write is staged on a credential the node checked,
            // and a provider that takes no delegation gives it nothing to
            // check a token against.
            None if defra_core::RequestContext::bound_or_ambient(self.context.as_ref())
                .transactional_authority_required() =>
            {
                Err(unavailable(
                    "this provider cannot verify a delegation for a verified write".into(),
                ))
            }
            // A provider that takes no delegation is passed its caller's token.
            None => Ok(acp::OwedAuthority::Delegation { token }),
        }
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

/// A provider's failure as document ACP reports it. One that says the
/// provider was not reached keeps saying so, since a caller may act on it.
fn provider_err(error: impl std::borrow::Borrow<ProviderError>) -> acp::Error {
    match error.borrow() {
        ProviderError::Unavailable(reason) => acp::Error::ProviderUnavailable(reason.clone()),
        ProviderError::RelationshipObservationDenied => acp::Error::NotManager {
            operation: "observe relationship".into(),
        },
        other => acp::Error::Storage(format!("Vera: {other}")),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl acp::RemotePolicies for VeraDocumentACP {
    async fn verify_transport_sentinel(
        &self,
        query: &acp::TransportSentinelQuery,
    ) -> std::result::Result<bool, acp::AssignmentReadError> {
        self.provider
            .verify_transport_sentinel(query)
            .await
            .map_err(|error| match error {
                ProviderError::Unavailable(_) | ProviderError::ProofsOvertaken(_) => {
                    acp::AssignmentReadError::Unavailable(error.to_string())
                }
                ProviderError::Unsupported(_) => acp::AssignmentReadError::Unsupported,
                _ => acp::AssignmentReadError::Invalid(error.to_string()),
            })
    }
    async fn certified_assignments(
        &self,
        query: &acp::AssignmentQuery,
    ) -> std::result::Result<acp::CertifiedAssignments, acp::AssignmentReadError> {
        let floor = query
            .not_before
            .as_ref()
            .map(|position| crate::AssignmentPosition {
                term: position.term,
                height: position.height,
                block_hash: position.block_hash.clone(),
            });
        let result = self
            .provider
            .read_certified_assignments(crate::AssignmentQuery {
                policy: &query.policy,
                resource: &query.resource,
                relation: &query.relation,
                subject: &query.subject,
                issuer: &query.issuer,
                not_before: floor.as_ref(),
            })
            .await
            .map_err(|error| match error {
                ProviderError::Unavailable(_) | ProviderError::ProofsOvertaken(_) => {
                    acp::AssignmentReadError::Unavailable(error.to_string())
                }
                ProviderError::Unsupported(_) => acp::AssignmentReadError::Unsupported,
                _ => acp::AssignmentReadError::Invalid(error.to_string()),
            })?;
        Ok(acp::CertifiedAssignments {
            object_ids: result.object_ids,
            position: acp::AssignmentPosition {
                term: result.position.term,
                height: result.position.height,
                block_hash: result.position.block_hash,
            },
            timestamp: result.timestamp,
            module_state_root: result.module_state_root,
        })
    }
    async fn add_policy(
        &self,
        creator: &str,
        policy: &str,
        nonce: Option<acp::PolicyNonce>,
    ) -> std::result::Result<String, String> {
        VeraDocumentACP::add_policy(self, creator, policy, nonce)
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
            chain_time: status.chain_time,
            module_state_root: status.module_state_root,
            cache_entries: status.cache_entries,
            last_invalidation_height: status.last_invalidation_height,
            connected: status.connected,
            delegation: self.provider.delegation_audience(),
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentACP for VeraDocumentACP {
    async fn verify_transport_sentinel(&self, query: &acp::TransportSentinelQuery) -> Result<bool> {
        self.provider
            .verify_transport_sentinel(query)
            .await
            .map_err(provider_err)
    }
    fn caller_of_delegation(
        &self,
        token: &str,
    ) -> Option<std::result::Result<acp::AuthenticatedCaller, String>> {
        Some(
            self.provider
                .caller_of_delegation(token)?
                .map_err(|error| error.to_string()),
        )
    }

    fn bind_request(&self, context: &defra_core::RequestContext) -> Option<Arc<dyn DocumentACP>> {
        Some(Arc::new(Self {
            provider: Arc::clone(&self.provider),
            decisions: self.decisions.clone(),
            context: Some(context.clone()),
        }))
    }

    fn bearer_token_for(&self, did: &str) -> Option<String> {
        defra_core::RequestContext::bound_or_ambient(self.context.as_ref()).bearer_token_for(did)
    }

    fn canonical_actor(&self, did: &Did) -> Did {
        Did::new_unchecked(self.provider.canonical_actor(did.as_str()))
    }

    fn own_actor(&self) -> Option<Did> {
        self.provider
            .self_did()
            .map(|did| Did::new_unchecked(self.provider.canonical_actor(&did)))
    }

    fn submits_subject(&self, subject: &Subject) -> bool {
        match subject {
            Subject::Entity(_) | Subject::Wildcard => true,
            Subject::EntitySet { .. } => self.provider.takes_subject_commands(),
            _ => false,
        }
    }

    fn owed_authority(&self, actor: &Did) -> Result<acp::OwedAuthority> {
        self.authority(actor, None)
    }

    fn command_authority(
        &self,
        actor: &Did,
        command: acp::ProviderCommand<'_>,
    ) -> Result<acp::OwedAuthority> {
        request_bound::with_policy_command(command, |command| self.authority(actor, command))
    }

    async fn submit_owed_change(&self, change: &acp::OwedChange) -> acp::Submission {
        owed::submit(self, change).await
    }

    async fn observe_actor_relationship(
        &self,
        observation: acp::ActorRelationshipObservation<'_>,
    ) -> Result<bool> {
        if observation.requester.is_wildcard() || observation.target.is_wildcard() {
            return Err(acp::Error::PermissionDenied(
                "an explicit actor DID is required".into(),
            ));
        }
        let [requester, actor, policy, resource, object, relation] = owned([
            observation.requester.as_str(),
            observation.target.as_str(),
            observation.policy,
            observation.resource,
            observation.document,
            observation.relation,
        ]);
        self.remote(move |provider| async move {
            provider
                .query_actor_relationship(ActorRelationshipQuery {
                    requester: &requester,
                    actor: &actor,
                    policy: &policy,
                    resource: &resource,
                    object: &object,
                    relation: &relation,
                })
                .await
        })
        .await
        .map_err(provider_err)
    }

    /// The provider's journal, when its worker keeps one, behind this ACP
    /// bound to no request: owed changes are sent with what was recorded
    /// with them, never with a request's credentials.
    fn owed_journal(&self) -> Option<Arc<dyn acp::OwedJournal>> {
        if !self.provider.journals_owed_requests() {
            return None;
        }
        let acp = Arc::new(Self {
            provider: Arc::clone(&self.provider),
            decisions: self.decisions.clone(),
            context: None,
        });
        Some(Arc::new(owed_journal::VeraOwedJournal { acp }))
    }

    async fn replica_group_member(&self, group: &acp::ReplicaGroup, signer: &Did) -> Result<bool> {
        let (group, signer) = (group.clone(), signer.clone());
        self.remote(
            move |provider| async move { provider.replica_group_member(&group, &signer).await },
        )
        .await
        .map_err(provider_err)
    }

    async fn check_named_permission(
        &self,
        subject: &Did,
        permission: &str,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        let [policy, resource, object, permission, actor] = owned([
            policy_id,
            resource_name,
            doc_id,
            permission,
            subject.as_str(),
        ]);
        self.remote(move |provider| async move {
            provider
                .verify_access(&policy, &resource, &object, &permission, &actor)
                .await
        })
        .await
        .map_err(provider_err)
    }

    fn unregistered_documents_are_public(&self) -> bool {
        self.provider.unregistered_documents_are_public()
    }

    fn proves_decisions(&self) -> bool {
        self.provider.proves_decisions()
    }

    async fn register_doc_object(
        &self,
        identity: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        let bearer_token = self.create_bearer_token(identity.as_str()).await?;
        self.admit_for(
            identity.as_str(),
            &bearer_token,
            &PolicyCommand {
                policy_id,
                resource: resource_name,
                object_id: doc_id,
                action: PolicyAction::RegisterObject,
            },
        )?;
        let [policy, resource, doc] = owned([policy_id, resource_name, doc_id]);
        self.remote(move |provider| async move {
            provider
                .register_object(&bearer_token, &policy, &resource, &doc)
                .await
        })
        .await
        .map_err(provider_err)?;
        Ok(())
    }

    async fn registration_parent(
        &self,
        policy_id: &str,
        child: &str,
        id: &str,
    ) -> Result<acp::RegistrationParent> {
        self.registration_parent_rule(policy_id, child, id)
            .await
            .map(|(parent, _)| parent)
    }

    async fn registration_parent_rule(
        &self,
        policy_id: &str,
        child: &str,
        id: &str,
    ) -> Result<(acp::RegistrationParent, String)> {
        let policy = self
            .get_policy(policy_id)
            .await
            .map_err(provider_err)?
            .ok_or_else(|| acp::Error::PolicyNotFound(policy_id.into()))?;
        acp::RegistrationParent::with_requirement(&policy, child, id)
            .map(|(parent, permission)| (parent, permission.to_owned()))
    }

    async fn register_under_parent(
        &self,
        identity: &Did,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        parent: &acp::RegistrationParent,
    ) -> Result<()> {
        let token = self.create_bearer_token(identity.as_str()).await?;
        self.admit_for(
            identity.as_str(),
            &token,
            &PolicyCommand {
                policy_id,
                resource,
                object_id,
                action: PolicyAction::RegisterUnderParent {
                    resource: &parent.resource,
                    object_id: &parent.object_id,
                },
            },
        )?;
        let [policy, resource, object] = owned([policy_id, resource, object_id]);
        let parent = parent.clone();
        self.remote(move |provider| async move {
            provider
                .register_under_parent(&token, &policy, &resource, &object, &parent)
                .await
        })
        .await
        .map_err(provider_err)?;
        Ok(())
    }

    async fn register_objects(
        &self,
        identity: &Did,
        objects: &[acp::AcpObject<'_>],
    ) -> std::result::Result<(), acp::RegistrationFailure> {
        self.register_listed(identity, objects).await
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

        self.decisions
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
                other => acp::Error::Storage(format!("Vera: {other}")),
            })
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
        self.admit_for(
            requestor.as_str(),
            &bearer_token,
            &PolicyCommand {
                policy_id,
                resource: resource_name,
                object_id: doc_id,
                action: PolicyAction::SetRelationship {
                    relation,
                    subject: PolicySubject::Actor(&subject),
                },
            },
        )?;
        let [policy, resource, doc, relation] = owned([policy_id, resource_name, doc_id, relation]);
        let result = self
            .remote(move |provider| async move {
                provider
                    .set_relationship(&bearer_token, &policy, &resource, &doc, &relation, &subject)
                    .await
            })
            .await
            .map_err(provider_err)?;

        Ok(result)
    }

    fn stages_archive_on_delete(&self) -> bool {
        self.provider.archives_owed_on_delete()
    }

    async fn archive_doc_object(
        &self,
        actor: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        let bearer_token = self.create_bearer_token(actor.as_str()).await?;
        self.admit_for(
            actor.as_str(),
            &bearer_token,
            &PolicyCommand {
                policy_id,
                resource: resource_name,
                object_id: doc_id,
                action: PolicyAction::ArchiveObject,
            },
        )?;
        let [policy, resource, doc] = owned([policy_id, resource_name, doc_id]);
        self.remote(move |provider| async move {
            provider
                .archive_object(&bearer_token, &policy, &resource, &doc)
                .await
        })
        .await
        .map_err(provider_err)?;
        Ok(())
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
        self.admit_for(
            requestor.as_str(),
            &bearer_token,
            &PolicyCommand {
                policy_id,
                resource: resource_name,
                object_id: doc_id,
                action: PolicyAction::DeleteRelationship {
                    relation,
                    subject: PolicySubject::Actor(&subject),
                },
            },
        )?;
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
            // A userset or cross-object subject is decided on the
            // requestor's authority, as an actor subject is.
            Subject::EntitySet { .. } => {
                let (kind, sr, so, srel) =
                    zanzibar::encode_subject(&target).map_err(acp::Error::from)?;
                let bearer_token = self.create_bearer_token(requestor.as_str()).await?;
                self.admit_for(
                    requestor.as_str(),
                    &bearer_token,
                    &PolicyCommand {
                        policy_id,
                        resource: resource_name,
                        object_id: doc_id,
                        action: PolicyAction::SetRelationship {
                            relation,
                            subject: PolicySubject::Structured {
                                kind,
                                resource: &sr,
                                object_id: &so,
                                relation: &srel,
                            },
                        },
                    },
                )?;
                let [policy, resource, doc, relation] =
                    owned([policy_id, resource_name, doc_id, relation]);
                let result = self
                    .remote(move |provider| async move {
                        provider
                            .set_relationship_subject_as(
                                &bearer_token,
                                &policy,
                                &resource,
                                &doc,
                                &relation,
                                kind,
                                &sr,
                                &so,
                                &srel,
                            )
                            .await
                    })
                    .await
                    .map_err(provider_err)?;
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
            // A userset or cross-object subject is decided on the
            // requestor's authority, as an actor subject is.
            Subject::EntitySet { .. } => {
                let (kind, sr, so, srel) =
                    zanzibar::encode_subject(&target).map_err(acp::Error::from)?;
                let bearer_token = self.create_bearer_token(requestor.as_str()).await?;
                self.admit_for(
                    requestor.as_str(),
                    &bearer_token,
                    &PolicyCommand {
                        policy_id,
                        resource: resource_name,
                        object_id: doc_id,
                        action: PolicyAction::DeleteRelationship {
                            relation,
                            subject: PolicySubject::Structured {
                                kind,
                                resource: &sr,
                                object_id: &so,
                                relation: &srel,
                            },
                        },
                    },
                )?;
                let [policy, resource, doc, relation] =
                    owned([policy_id, resource_name, doc_id, relation]);
                let result = self
                    .remote(move |provider| async move {
                        provider
                            .delete_relationship_subject_as(
                                &bearer_token,
                                &policy,
                                &resource,
                                &doc,
                                &relation,
                                kind,
                                &sr,
                                &so,
                                &srel,
                            )
                            .await
                    })
                    .await
                    .map_err(provider_err)?;
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

        // The node archives as its own actor where the provider names one,
        // else as the object's owner.
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

        Ok(())
    }
}

mod owed;
mod owed_journal;
mod registration_batch;
mod request_bound;

pub(crate) use owed_journal::MAX_OWED_REQUEST_BYTES;

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

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;

#[non_exhaustive]
pub enum SubjectRef {
    Actor(String),
    AllActors,
}

pub struct ProviderPolicyInfo {
    pub id: String,
    pub name: String,
    pub raw_policy: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AcpLightClientStatus {
    pub height: u64,
    pub module_state_root: String,
    pub cache_entries: usize,
    pub last_invalidation_height: u64,
    pub connected: bool,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("query error: {0}")]
    Query(String),

    #[error("transaction error: {0}")]
    Transaction(String),

    #[error("config error: {0}")]
    Config(String),

    /// Vera is temporarily unreachable (circuit breaker open or timeout).
    /// All access decisions must fail-closed when this is returned.
    #[error("Vera unavailable: {0}")]
    Unavailable(String),

    /// The provider backend does not support the requested operation.
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait VeraProvider: MaybeSendSync {
    /// Whether missing registration permits access without evaluating the policy.
    fn unregistered_documents_are_public(&self) -> bool {
        false
    }

    /// Whether deleting a DefraDB document should archive the corresponding
    /// object in the provider's shared authorization state.
    ///
    /// The default preserves the existing archive-on-delete behavior.
    /// Providers with shared, replicated authorization state may
    /// return `false`: a local tombstone can still be rejected by another
    /// replica, so archiving here could revoke access before that decision.
    /// Explicit provider archive operations remain available independently.
    fn archive_object_on_document_delete(&self) -> bool {
        true
    }

    fn authorized_account(&self) -> String;

    async fn create_bearer_token(&self, did: &str) -> Result<String, ProviderError>;

    /// [`Self::create_bearer_token`] for `did` acting in the request
    /// `context` serves, which may carry the token `did` authenticated with
    /// ([`RequestContext::bearer_token_for`](defra_core::RequestContext::bearer_token_for)).
    async fn create_bearer_token_as(
        &self,
        _context: &defra_core::RequestContext,
        did: &str,
    ) -> Result<String, ProviderError> {
        self.create_bearer_token(did).await
    }

    fn self_did(&self) -> Option<String>;

    async fn create_policy(&self, policy_yaml: &str) -> Result<String, ProviderError>;

    async fn register_object(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<(), ProviderError>;

    async fn archive_object(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<(), ProviderError>;

    async fn set_relationship(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &SubjectRef,
    ) -> Result<bool, ProviderError>;

    async fn delete_relationship(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &SubjectRef,
    ) -> Result<bool, ProviderError>;

    /// Emit a structured cross-object (object-edge) or userset subject to the
    /// backend, routed by `kind` per the frozen cross-object wire contract
    /// (`kind` 2 = object edge, `kind` 3 = userset). The `subject_*` fields are
    /// the codec output of [`zanzibar::encode_subject`].
    ///
    /// Authentication is by the provider's own signing key (no bearer token);
    /// only backends that implement the typed precompile override this. The
    /// default rejects with [`ProviderError::Unsupported`].
    #[allow(clippy::too_many_arguments)]
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
        let _ = (
            policy_id,
            resource,
            object_id,
            relation,
            kind,
            subject_resource,
            subject_object_id,
            subject_relation,
        );
        Err(ProviderError::Unsupported(
            "structured cross-object/userset subjects are not supported by this provider".into(),
        ))
    }

    /// Remove a structured cross-object or userset subject. Revoke mirror of
    /// [`set_relationship_subject`](Self::set_relationship_subject); the default
    /// rejects with [`ProviderError::Unsupported`].
    #[allow(clippy::too_many_arguments)]
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
        let _ = (
            policy_id,
            resource,
            object_id,
            relation,
            kind,
            subject_resource,
            subject_object_id,
            subject_relation,
        );
        Err(ProviderError::Unsupported(
            "structured cross-object/userset subjects are not supported by this provider".into(),
        ))
    }

    async fn query_policy(
        &self,
        policy_id: &str,
    ) -> Result<Option<ProviderPolicyInfo>, ProviderError>;

    async fn query_object_owner(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<(bool, String), ProviderError>;

    async fn verify_access(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        permission: &str,
        actor_did: &str,
    ) -> Result<bool, ProviderError>;

    async fn create_access_decision(
        &self,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        permission: &str,
        actor_did: &str,
    ) -> Result<Option<String>, ProviderError> {
        let _ = (policy_id, resource, object_id, permission, actor_did);
        Ok(None)
    }

    /// The verified ACP state a decision is read at, when the provider
    /// proves one and it is still fresh (the vera.rs light client's module
    /// state root, within its freshness limit). A decision cached under it
    /// is exact: a new state is a new key, and a state that aged out is no
    /// key at all.
    fn decision_state(&self) -> Option<String> {
        None
    }

    fn acp_light_client_status(&self) -> Result<AcpLightClientStatus, ProviderError> {
        Err(ProviderError::Unavailable(
            "ACP light client status is not available for this provider".into(),
        ))
    }

    /// What a caller's own delegation must name for this provider to act
    /// under it; `None` when the provider takes none.
    fn delegation_audience(&self) -> Option<acp::DelegationAudience> {
        None
    }

    /// The DID that signed `token`, when `token` is a delegation this
    /// provider may act under now: signed by its issuer, current, and naming
    /// this provider's worker and deployment. `None` when `token` is not a
    /// delegation at all, so the caller is authenticated some other way.
    fn caller_of_delegation(&self, _token: &str) -> Option<Result<String, ProviderError>> {
        None
    }
}

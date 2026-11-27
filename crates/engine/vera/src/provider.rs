use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;

pub use vera_delegation::SubjectRef;

pub struct ProviderPolicyInfo {
    pub id: String,
    pub name: String,
    pub raw_policy: Option<String>,
}

#[derive(Clone, Copy)]
pub struct ActorRelationshipQuery<'a> {
    pub requester: &'a str,
    pub policy: &'a str,
    pub resource: &'a str,
    pub object: &'a str,
    pub relation: &'a str,
    pub actor: &'a str,
}

/// Select direct relationship assignments for one subject and issuer.
#[derive(Clone, Copy)]
pub struct AssignmentQuery<'a> {
    pub policy: &'a str,
    pub resource: &'a str,
    pub relation: &'a str,
    pub subject: &'a str,
    pub issuer: &'a str,
    /// Reject evidence older than this previously verified position.
    /// A verified newer term is accepted, including when its height resets.
    pub not_before: Option<&'a AssignmentPosition>,
}

/// A finalized block position a consumer can persist between reads.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssignmentPosition {
    pub term: u64,
    pub height: u64,
    pub block_hash: String,
}

/// A complete verified assignment set and the evidence that authenticates it.
/// The evidence can include other subjects' relationships in the same policy.
#[derive(Debug, Clone)]
pub struct CertifiedAssignments {
    pub object_ids: Vec<String>,
    pub position: AssignmentPosition,
    pub timestamp: u64,
    pub module_state_root: String,
    pub evidence: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct AcpLightClientStatus {
    pub height: u64,
    /// The block time of the newest finalized revision the light client
    /// verified, in Unix seconds.
    pub chain_time: Option<u64>,
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

    /// Vera is temporarily unreachable (a timeout or a failed connection).
    /// All access decisions must fail-closed when this is returned.
    #[error("Vera unavailable: {0}")]
    Unavailable(String),

    /// The provider backend does not support the requested operation.
    #[error("unsupported operation: {0}")]
    Unsupported(String),

    #[error("relationship observation is not authorized")]
    RelationshipObservationDenied,
    /// Two proofs of one observation were read at different certified
    /// roots: a block finalized between them. Nothing failed; asked again,
    /// the observation is answered at one root.
    #[error("{0} and policy proofs have different roots")]
    ProofsOvertaken(&'static str),
    /// A delegation does not hold at the provider's time; one issued at
    /// that time is taken.
    #[error("delegation refused: {0}")]
    DelegationOutsideWindow(acp::DelegationWindow),
}

/// A command that cannot be typed, encoded or digested, with the text the
/// provider always gave it: a policy identifier that names no policy is a
/// query error, anything else a configuration error.
impl From<vera_delegation::CommandError> for ProviderError {
    fn from(error: vera_delegation::CommandError) -> Self {
        match error {
            vera_delegation::CommandError::PolicyId(_) => Self::Query(error.to_string()),
            other => Self::Config(other.to_string()),
        }
    }
}

/// The most registrations one request carries: the most commands vera.rs
/// applies in one ordered list.
pub const MAX_REGISTRATIONS_PER_REQUEST: usize = vera_modules::acp::MAX_POLICY_COMMAND_LIST;

/// One command of a request built for owed changes, under the bearer token
/// it is submitted with.
pub struct OwedCall<'a> {
    pub bearer_token: &'a str,
    pub command: crate::PolicyCommand<'a>,
}

/// What a request journaled for owed changes came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwedRequestOutcome {
    /// It executed and every command in it applied.
    Applied,
    /// It executed and was rejected: nothing in it applied.
    Rejected(String),
    /// Neither the journal, a verified receipt nor the note knows it.
    Unknown,
}

/// The name a provider gives the request `request`: the hex Keccak-256
/// digest of its bytes, as stored with the changes it carries and as the
/// note of resolved requests keys it.
pub fn owed_request_digest(request: &[u8]) -> String {
    format!("{:#x}", alloy_primitives::keccak256(request))
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

    /// Whether a document's delete archives its object on this provider as
    /// an owed change staged in the delete's transaction and made as the
    /// object's owner, whoever deletes
    /// (`acp::TransactionOutbox::stage_archive`). Such a provider archives
    /// nothing after the commit ([`Self::archive_object_on_document_delete`]
    /// is `false` for it).
    fn archives_owed_on_delete(&self) -> bool {
        false
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

    /// `did` in the one spelling this provider keeps and returns an actor
    /// under, for a provider whose actors can be written in several.
    fn canonical_actor(&self, did: &str) -> String {
        did.to_owned()
    }

    /// Whether [`Self::create_bearer_token`] signs for `did` with a key the
    /// node holds, so it needs nothing from a request to act as `did`. The
    /// answer is the same for every spelling [`Self::canonical_actor`]
    /// brings to one: a staged change keeps the canonical one only.
    fn holds_key_for(&self, _did: &str) -> bool {
        false
    }

    /// Whether a relationship whose subject is a userset or another object
    /// can be submitted to this provider
    /// ([`Self::set_relationship_subject_as`]).
    fn takes_subject_commands(&self) -> bool {
        false
    }

    /// Whether `error`, returned by a relationship write, is the provider's
    /// own final answer to that request (a verified rejection) and not a
    /// failure to reach it or to learn the outcome. The default knows no
    /// such error, so a failed write is taken as possibly not delivered.
    fn write_refused(&self, _error: &ProviderError) -> bool {
        false
    }

    /// Whether this provider's worker journals each request until a
    /// verified receipt lets it go, so owed changes can be sent through
    /// [`Self::journal_owed_request`] with an in-flight mark.
    fn journals_owed_requests(&self) -> bool {
        false
    }

    /// The request that submits `calls` in order, applied together or not
    /// at all: one command alone, several in one batch.
    fn owed_request(&self, calls: &[OwedCall<'_>]) -> Result<Vec<u8>, ProviderError> {
        let _ = calls;
        Err(ProviderError::Unsupported(
            "this provider journals no requests".into(),
        ))
    }

    /// The worker's next sequence: every request journaled from now on is
    /// numbered from it, and every one resolved before below it.
    async fn owed_request_floor(&self) -> Result<u64, ProviderError> {
        Err(ProviderError::Unsupported(
            "this provider journals no requests".into(),
        ))
    }

    /// Have the worker journal `request`, resolving first one it already
    /// journals (and noting that one's outcome), and return its transaction
    /// hash. A request the journal holds already is kept as it is.
    async fn journal_owed_request(&self, request: &[u8]) -> Result<String, ProviderError> {
        let _ = request;
        Err(ProviderError::Unsupported(
            "this provider journals no requests".into(),
        ))
    }

    /// What became of the request whose digest is `digest` and, once it
    /// was journaled, whose transaction hash is `tx`: one still journaled is
    /// sent and its verified receipt waited for, one out of the journal is
    /// looked up by its hash, then in the note by its digest among the
    /// requests numbered from `floor` on.
    async fn owed_request_outcome(
        &self,
        digest: &str,
        tx: Option<&str>,
        floor: Option<u64>,
    ) -> Result<OwedRequestOutcome, ProviderError> {
        let _ = (digest, tx, floor);
        Err(ProviderError::Unsupported(
            "this provider journals no requests".into(),
        ))
    }

    /// Create the YAML policy as the provider's actor and return its ID.
    /// vera.rs derives the ID from the definition, the actor and `nonce`,
    /// drawing a random nonce when none is given; sent again with the same
    /// nonce it returns the policy already made and makes no second one.
    /// A provider whose ACP assigns its own IDs refuses a nonce.
    async fn create_policy(
        &self,
        policy_yaml: &str,
        nonce: Option<acp::PolicyNonce>,
    ) -> Result<String, ProviderError>;

    /// Register `object_id` with the token's actor as its owner, together
    /// with `relationships`, what the document carries through its
    /// `@policy(relation:)` fields: the provider checks the collection's
    /// `create` over them and stores them with the owner.
    async fn register_object(
        &self,
        bearer_token: &str,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relationships: &[acp::DerivedRelationship],
    ) -> Result<(), ProviderError>;

    /// Register each of `objects` under `bearer_token` in one request the
    /// provider applies in order, all or none, at most
    /// [`MAX_REGISTRATIONS_PER_REQUEST`] of them. A provider that applies no
    /// such list refuses it as unsupported, and its registrations are sent
    /// one at a time ([`Self::register_object`]).
    async fn register_objects(
        &self,
        bearer_token: &str,
        objects: &[acp::AcpObject<'_>],
    ) -> Result<(), ProviderError> {
        let _ = (bearer_token, objects);
        Err(ProviderError::Unsupported(
            "this provider registers one object per request".into(),
        ))
    }

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

    /// [`set_relationship_subject`](Self::set_relationship_subject) under
    /// `bearer_token`: the command carries its caller's authority, as an
    /// actor subject's does, and the backend decides on it. The default
    /// rejects with [`ProviderError::Unsupported`].
    #[allow(clippy::too_many_arguments)]
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
        let _ = (
            bearer_token,
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

    /// Revoke mirror of
    /// [`set_relationship_subject_as`](Self::set_relationship_subject_as).
    #[allow(clippy::too_many_arguments)]
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
        let _ = (
            bearer_token,
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

    /// Read complete assignment evidence. Callers must authorize its disclosure.
    async fn read_certified_assignments(
        &self,
        _query: AssignmentQuery<'_>,
    ) -> Result<CertifiedAssignments, ProviderError> {
        Err(ProviderError::Unsupported(
            "certified assignment discovery".into(),
        ))
    }

    async fn query_actor_relationship(
        &self,
        _query: ActorRelationshipQuery<'_>,
    ) -> Result<bool, ProviderError> {
        Err(ProviderError::Unsupported(
            "verified exact actor relationship observation".into(),
        ))
    }

    async fn verify_transport_sentinel(
        &self,
        _query: &acp::TransportSentinelQuery,
    ) -> Result<bool, ProviderError> {
        Err(ProviderError::Unsupported(
            "verified transport sentinel".into(),
        ))
    }

    async fn replica_group_member(
        &self,
        _group: &acp::ReplicaGroup,
        _signer: &identity::Did,
    ) -> Result<bool, ProviderError> {
        Err(ProviderError::Unsupported(
            "certified replica group membership".into(),
        ))
    }

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

    /// Whether [`Self::verify_access`] proves every decision against the
    /// provider's finalized state at a revision within its freshness limit,
    /// and fails when it cannot: the vera.rs light client does.
    fn proves_decisions(&self) -> bool {
        false
    }

    fn acp_light_client_status(&self) -> Result<AcpLightClientStatus, ProviderError> {
        Err(ProviderError::Unavailable(
            "ACP light client status is not available for this provider".into(),
        ))
    }

    /// Where this provider hands back the registrations and relationship
    /// changes it confirmed on blocks a switch of its deployment's active
    /// device voided, to be owed again. A provider without terms takes none.
    fn owe_voided_to(&self, _sink: std::sync::Weak<dyn acp::OweAgain>) {}

    /// Give the provider the node's own access-state source. A takeover that
    /// may have voided more than the journal keeps rebuilds from it instead
    /// of a new worker.
    fn set_recovery_source(&self, _source: std::sync::Weak<dyn acp::RecoverySource>) {}

    /// What a caller's own delegation must name for this provider to act
    /// under it, with the deployment's genesis identity when the provider
    /// knows it; `None` when the provider takes none.
    fn delegation_audience(&self) -> Option<acp::DelegationAudience> {
        None
    }

    /// The DID that signed `token`, when `token` is a delegation this
    /// provider may act under now: signed by its issuer, current, and naming
    /// this provider's worker and deployment. `None` when `token` is not a
    /// delegation at all, so the caller is authenticated some other way.
    fn caller_of_delegation(
        &self,
        _token: &str,
    ) -> Option<Result<acp::AuthenticatedCaller, ProviderError>> {
        None
    }

    /// The DID that signed `token`, when `token` is a delegation bound to
    /// one request and this provider may send `command` under it now: signed
    /// by its issuer, current, naming this provider's worker and deployment
    /// and, when the provider knows it, the deployment's genesis identity,
    /// and bound to exactly `command`. An error when it is bound to another
    /// command or names another genesis identity. `None` when `token` is bound to no request, or this provider
    /// takes no such delegation: [`Self::caller_of_delegation`] then says
    /// who signed it.
    fn caller_of_request_delegation(
        &self,
        _token: &str,
        _command: &crate::PolicyCommand<'_>,
    ) -> Option<Result<String, ProviderError>> {
        None
    }
}

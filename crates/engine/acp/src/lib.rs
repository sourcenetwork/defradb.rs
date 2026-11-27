//! Access Control Policy (ACP) for DefraDB
//!
//! Document access control (DAC) and node access control (NAC). NAC and the
//! local DAC are evaluated by the `zanzibar` engine over relationship tuples;
//! a Vera DAC is decided by the chain.
//!
//! # Architecture
//!
//! - `DocumentACP` trait: the document-scope interface (register, check, share, revoke)
//! - `ZanzibarDocumentACP`: the node's local DAC, with its tuples in the node's store
//! - `ExternalDocumentAcp`: the gate around a remote provider (Vera)
//! - `LocalDocumentACP`: a DAC over an `AcpStore` such as `MemoryAcpStore`, for tests
//! - `nac`: the node's NAC policy and its lifecycle
//! - `policy_yaml`: parses and checks a DAC policy before it is stored
//!
//! # Public vs Registered Documents
//!
//! - Document created without an identity: public, anyone can access it
//! - Document created with an identity: registered, its creator is the owner,
//!   and the policy decides who else may read, update or delete it
//!
//! # Policy rules
//!
//! - `owner` is reserved: every resource gets an `owner` relation, and a policy
//!   that declares one is refused (``'owner` is a reserved relation name``)
//! - The owner holds every permission: each expression is joined to `owner`
//!   by union, an empty expression grants the owner alone, and an expression
//!   may not name the resource's own `owner` (``permission cannot reference
//!   `owner` relation``); `parent->owner` is accepted
//! - Under `spec: defra` every resource needs `read` and `write`, and `read`
//!   is joined to `write`, so an empty `read` also grants the `write` holders
//! - Expressions use `+` (union), `-` (difference), `&` (intersection) and
//!   `relation->permission` (follow `relation` to the object it names and
//!   check `permission` there), with equal precedence, left to right, and
//!   parentheses to group: the grammar of acp_core's `PermissionExpr.g4`
//! - A resource a collection links must have `read`, `update` and `delete`
//!   permissions

mod auth_error;
mod commit_registrations;
mod dac;
mod delegation_audience;
mod delegation_window;
pub mod error;
mod external;
mod identity;
mod local;
pub mod nac;
mod object_registration;
pub mod outbox;
pub mod pending;
mod permission;
mod policy_cache;
mod policy_lookup;
mod policy_nonce;
pub mod projection;
mod provider_command;
pub mod recovery;
pub mod replica;
// Both persistent stores require `S: Store + Send + Sync`, which the wasm
// LevelDB store cannot satisfy, so they are native-only.
#[cfg(not(target_arch = "wasm32"))]
mod persistent;
pub mod policy_yaml;
pub mod read_access;
mod relation;
mod store;
mod target_subject;
mod validation;
pub mod zanzibar;

pub use auth_error::normalize_auth_error;
pub use commit_registrations::{CommitRegistration, CommitRegistrations};
pub use dac::{
    bound_to_request, with_request_bearer, ActorRelationshipObservation, AuthenticatedCaller,
    DocumentACP, DocumentRelationship, ObjectChangeListener, OwedRefusalListener,
};
pub use delegation_audience::DelegationAudience;
pub use delegation_window::{DelegationWindow, WindowRefusal, CLOCK_SKEW_ALLOWANCE};
pub use error::{Error, Result};
pub use external::{
    ExternalDocumentAcp, OweAgain, OwedScope, OwedSettlement, RedriveReport, VoidedEffect,
    MAX_CHANGES_PER_REQUEST, MAX_FAILED_SUBMISSIONS, OBSERVATION_PATIENCE,
};
pub use identity::Identity;
pub use local::{LocalDocumentACP, MemoryAcpStore};
pub use object_registration::{AcpObject, RegistrationFailure, RegistrationParent};
pub use outbox::{
    InFlight, NewOwedChange, OwedAuthority, OwedAuthorityKind, OwedChange, OwedChanges, OwedDenial,
    OwedDenials, OwedEntry, OwedFilter, OwedHold, OwedJournal, OwedOp, OwedOutcome, OwedRef,
    OwedState, OwedSubject, OwedTuple, OwedTupleState, OwedWait, OwedWhat, PreparedRequest,
    RequestOutcome, StageArchive, StageRelationship, Staged, StagedCount, Submission,
    TransactionOutbox,
};
pub use pending::{PendingRegistration, PendingRegistrations};
pub use permission::DocumentPermission;
#[cfg(not(target_arch = "wasm32"))]
pub use persistent::PersistentAcpStore;
pub use policy_lookup::{
    AssignmentPosition, AssignmentQuery, AssignmentReadError, CertifiedAssignments,
    LightClientStatus, PolicyLookup, RemotePolicies, StorePolicies, TransportSentinelQuery,
};
pub use policy_nonce::{PolicyNonce, PolicyNonceError, POLICY_NONCE_BYTES};
pub use provider_command::ProviderCommand;
pub use read_access::{check_doc_read_access, DirectChecker, DocAccess, ObjectAccessChecker};
pub use recovery::{
    Observed, RecoveryCursor, RecoveryPage, RecoverySource, RecoveryStep, RecoveryVerifier,
};
pub use relation::{
    RelationTuple, DELETER_RELATION, OWNER_RELATION, READER_RELATION, UPDATER_RELATION,
};
pub use replica::{ReplicaGrant, ReplicaGroup, ReplicaOwed, ReplicaUnit};
pub use store::AcpStore;
pub use target_subject::parse_target_subject;
pub use validation::{validate_resource_interface, REQUIRED_DOCUMENT_PERMISSIONS};

// Re-export key zanzibar engine types from the standalone zanzibar crate
pub use zanzibar::PersistentZanzibarStore;
pub use zanzibar::ZanzibarDocumentACP;

pub use ::zanzibar::store::{ScopeAttempt, ScopeOutcome, ScopeValue};
pub use ::zanzibar::{
    EvaluationStep, EvaluationTrace, MemoryZanzibarStore, PermissionCheckRequest, PermissionEngine,
    PermissionExplanation, Policy, Relation, RelationExpression, Relationship, Resource,
    StepResult, StorePolicyOptions, Subject, SubjectRestriction, ZanzibarStore,
};

// Re-export NAC types
pub use nac::{
    create_node_policy, is_valid_nac_relation, validate_node_policy, NacStatus, NodeACP,
    NodeAcpOperations, NodePermission, ADMIN_RELATION as NAC_ADMIN_RELATION, NODE_OBJECT_ID,
    NODE_POLICY_ID, NODE_RESOURCE_NAME, OWNER_RELATION as NAC_OWNER_RELATION, VALID_NAC_RELATIONS,
};

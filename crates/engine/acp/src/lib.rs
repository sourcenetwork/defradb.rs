//! Access Control Policy (ACP) for DefraDB
//!
//! This crate provides document-level access control following Go DefraDB's pattern:
//! 1. If ACP not configured -> allow all
//! 2. Check if peer is replicator -> allow (fast path)
//! 3. Verify peer's identity token
//! 4. Check document-level ACP permissions -> allow/deny
//!
//! # Architecture
//!
//! - `DocumentACP` trait: Core interface for document access control
//! - `LocalDocumentACP`: Local implementation using in-memory storage
//! - `DocumentPermission`: Read/Update/Delete permissions
//! - `RelationTuple`: Subject-relation-object tuples for permission storage
//!
//! # Public vs Registered Documents
//!
//! - Document created WITHOUT identity -> public (unregistered) -> anyone can access
//! - Document created WITH identity -> registered -> creator is owner -> ACP enforced
//!
//! # DPI Rules (DefraDB Policy Interface)
//!
//! - Every resource MUST have `owner` relation
//! - Every permission expression MUST include `owner` access
//! - Difference (`-`) and tuple-to-userset (`->`) are allowed
//! - Intersection (`&`) is not allowed in DPI-enforced policies

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
pub use object_registration::{AcpObject, RegistrationFailure};
pub use outbox::{
    InFlight, NewOwedChange, OwedAuthority, OwedAuthorityKind, OwedChange, OwedChanges, OwedDenial,
    OwedDenials, OwedEntry, OwedFilter, OwedHold, OwedJournal, OwedOp, OwedOutcome, OwedRef,
    OwedState, OwedSubject, OwedTuple, OwedTupleState, OwedWait, OwedWhat, PreparedRequest,
    RequestOutcome, StageRelationship, Staged, StagedCount, Submission, TransactionOutbox,
};
pub use pending::{PendingRegistration, PendingRegistrations};
pub use permission::DocumentPermission;
#[cfg(not(target_arch = "wasm32"))]
pub use persistent::PersistentAcpStore;
pub use policy_lookup::{
    AssignmentPosition, AssignmentQuery, AssignmentReadError, CertifiedAssignments,
    LightClientStatus, PolicyLookup, RemotePolicies, StorePolicies,
};
pub use policy_nonce::{PolicyNonce, PolicyNonceError, POLICY_NONCE_BYTES};
pub use provider_command::ProviderCommand;
pub use read_access::{check_doc_read_access, DirectChecker, DocAccess, ObjectAccessChecker};
pub use relation::{
    RelationTuple, DELETER_RELATION, OWNER_RELATION, READER_RELATION, UPDATER_RELATION,
};
pub use replica::{ReplicaGrant, ReplicaOwed, ReplicaUnit};
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

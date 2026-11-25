//! Transport-agnostic P2P synchronization for DefraDB.
//!
//! This crate holds what every DefraDB peer shares, whatever carries its
//! bytes: the wire messages, the [`P2PTransport`] trait a transport
//! implements, and the sync coordinator that turns transport events into
//! merges. The `pinc` crate implements the transport.
//!
//! # Modules
//!
//! - [`protocol`] - Protocol constants and identifiers
//! - [`message`] - Wire message types (CBOR-encoded)
//! - [`transport`] - The transport trait and its events
//! - [`sync`] - The sync coordinator and manager
//! - [`topics`] - Topic definitions
//! - [`error`] - Error types

pub mod ack_scope;
pub mod bitswap;
pub mod error;
mod explicit_replay;
#[cfg(feature = "kms")]
pub mod kms;
pub mod manage_correlator;
pub mod message;
pub mod model_trace;
pub mod network;
pub mod peer_identity;
pub mod protocol;
#[cfg(feature = "kms")]
pub mod pubsub_rpc;
pub mod quiet_peers;
pub mod replicator;
pub mod rt;
pub mod se_correlator;
pub mod signing;
pub mod sync;
pub mod topics;
pub mod tracked_task;
pub mod transport;
#[cfg(test)]
mod transport_kind_tests;

// Re-export main types for convenience
pub use error::{Error, Result};
pub use explicit_replay::{
    generate_capability as generate_explicit_replay_capability,
    generate_capability_from_claims as generate_explicit_replay_capability_from_claims,
    is_capability_revoked as is_explicit_replay_capability_revoked,
    revoke_capability as revoke_explicit_replay_capability,
    verify_capability as verify_explicit_replay_capability,
    verify_capability_for_key_request as verify_explicit_replay_capability_for_key_request,
    verify_capability_with_revocations as verify_explicit_replay_capability_with_revocations,
    ExplicitReplayAuthorization, ExplicitReplayCapabilityCache, ExplicitReplayCapabilityClaims,
    ExplicitReplayRevocationRegistry,
    DEFAULT_CAPABILITY_TTL as DEFAULT_EXPLICIT_REPLAY_CAPABILITY_TTL,
    MAX_CAPABILITY_TTL as MAX_EXPLICIT_REPLAY_CAPABILITY_TTL,
};
pub use message::{Message, MetaData, PushLogBroadcast, PushLogReply, PushLogRequest};
pub use peer_identity::{AnonymousResolver, PeerIdentityResolver};
pub use protocol::{
    BASE_PROTOCOL_ID, CODE, MESSAGE_VERSION, NAME, PROTOCOL_BASE, REP_REQUEST_PROTOCOL,
    REP_RESPONSE_PROTOCOL, VERSION,
};

// Re-export deprecated aliases for backwards compatibility
#[allow(deprecated)]
pub use protocol::{PUSHLOG_REQUEST_PROTOCOL, PUSHLOG_RESPONSE_PROTOCOL};

// Re-export signing functions
pub use signing::sign_with_transport;

// Re-export topic types
pub use topics::{DefraTopic, DOC_SYNC_TOPIC, ENCRYPTION_TOPIC, SYNC_BRANCHABLE_TOPIC};

// Re-export transport types
pub use transport::{
    InboundClassSnapshot, P2PTransport, PeerConnectionInfo, PeerFetchStats, TransportEvent,
};

// Re-export sync types
pub use sync::{
    Broadcaster, CreateReplicatorResult, DagSync, DagSyncConfig, DagSyncState,
    LoadReplicatorsResult, NeedsFetchData, PeerHistory, PeerStateTracker, PeerSummary,
    ProcessQueue, SyncConfig, SyncCoordinator, SyncEvent, SyncManager, SyncPlan,
};
pub use sync::{
    BLOCK_SYNC_COMPLETION_WATCHDOG, CLOCK_ATTEMPT_QUANTUM, PENDING_RECOVERY_WORST_CASE_SECS,
};

pub use bitswap::{AccessMode, ReplicatorRegistry};

/// Query ID for tracking block sync operations.
/// This is a simple wrapper that allows correlating sync requests with completions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueryId(pub u64);

pub use ack_scope::AckScope;

// Re-export replicator types
pub use replicator::{
    EqOnlyFilterMatcher, ReplicationFilter, ReplicationFilterMatcher, ReplicationFilters,
    ReplicatorInfo, ReplicatorStatus,
};

// Re-export management correlators
pub use manage_correlator::{
    ManageCorrelator, ManageQueryCorrelator, PendingManage, PendingManageQuery,
};

// Re-export SE query correlator
pub use se_correlator::{PendingSeQuery, SeQueryCorrelator};

//! P2P synchronization module for DefraDB.
//!
//! This module provides block synchronization between DefraDB peers.
//! It handles:
//! - Receiving blocks from the network (via PushLog messages)
//! - Storing blocks in the blockstore with merge tracking
//! - Applying CRDT merges to integrate remote changes
//! - Broadcasting local changes to the network

use std::time::Duration;

mod broadcast_coalescer;
mod broadcaster;
pub(crate) mod car;
mod car_authorization;
mod collection_store;
mod coordinator;
mod dag_sync;
mod dispatch_queue;
mod event_dispatcher;
mod head_provider;
mod head_scope;
mod manager;

pub use manager::pending::PENDING_RECOVERY_WORST_CASE_SECS;
mod merge;
mod peer_state;
pub mod pending_store;
mod push_backlog;
mod queue;
pub(crate) mod rate_limiter;
mod replication;
pub mod shinzo_car;

pub use broadcaster::{BroadcastResult, Broadcaster};
pub use collection_store::{MemoryCollectionStorage, P2PCollectionStorage, P2PCollectionStore};
#[cfg(feature = "iroh-transport")]
pub use coordinator::IrohSyncCoordinator;
#[cfg(feature = "libp2p-transport")]
pub use coordinator::Libp2pSyncCoordinator;
pub use coordinator::{
    CreateReplicatorResult, HeadHintCarAuthority, HeadHintCarGrant, LoadReplicatorsResult,
    PeerDetails, PendingFetch, PushFailure, PushFailureReceiver, QuarantinedRoot, SyncCoordinator,
    SyncShutdownHandle, SyncStatus,
};
pub use dag_sync::{DagSync, DagSyncConfig, DagSyncState, NeedsFetchData, SyncPlan};
pub use dispatch_queue::{
    dispatch_queue, DispatchReceiver, DispatchSender, DispatchSource, Lease, Queued,
};
pub(crate) use event_dispatcher::classify_p2p_event;
pub(crate) use event_dispatcher::DispatchDiagnostics;
pub use event_dispatcher::{DispatchAdmission, DispatchClass, DispatchEvent, DispatchSnapshot};
pub use head_provider::{DocumentHeadProvider, NoOpHeadProvider};
#[cfg(any(feature = "libp2p-transport", feature = "iroh-transport"))]
pub(crate) use manager::record_gossip_decode_failure_sample;
pub use manager::{
    default_rate_limit_backoff, GossipDecodeFailureSample, GossipTransport, SyncConfig,
    SyncDiagnostics, SyncDiagnosticsSnapshot, SyncEvent, SyncEventReceiver, SyncManager,
    DEFAULT_MAX_ACTIVE_PUSHES_PER_PEER, DEFAULT_MAX_CONCURRENT_DAG_FETCHES,
    DEFAULT_MAX_CONCURRENT_PUSH_TASKS, DEFAULT_MAX_DOC_SYNC_REQUEST_DOC_IDS,
    DEFAULT_MAX_PENDING_DAGS, DEFAULT_PUSH_QUEUE_BYTE_CAPACITY, DEFAULT_PUSH_QUEUE_CAPACITY,
    DEFAULT_PUSH_SEND_TIMEOUT, DEFAULT_RATE_LIMIT_BACKOFF_SECS, DEFAULT_RATE_LIMIT_BURST,
    DEFAULT_RATE_LIMIT_RATE,
};
pub use merge::{
    BlockMetadata, MergeBlock, MergeErrorDisposition, MergeHandler, MergeOutcome,
    RecoveredBlockMetadata,
};
pub use peer_state::{
    PeerFailure, PeerHistory, PeerStateTracker, PeerStats, PeerSummary, SyncOutcomes, Timestamp,
};
pub use pending_store::{
    pooled_pending_store, Admission, PendingDagStorage, PendingDagStore, PersistedPendingDag,
    PersistedQuarantinedDag, PersistedReplayAuthorization, PersistedView, Registration,
};
pub use push_backlog::{
    EnqueueOutcome, PeerBacklogSnapshot, PushBacklog, PushBacklogSnapshot, PushJobSpec,
};
pub use queue::ProcessQueue;
pub use replication::{recover_unmerged, ReplicationConfig, ReplicationLoop, ReplicationResult};

/// Cadence for draining persisted push retries, and the tick the replicator
/// reconnect probe runs on, whose `RECONNECT_BACKOFF_MIN` floor derives from
/// this value.
pub const PERSISTED_RETRY_SWEEP_INTERVAL: Duration = Duration::from_secs(2);

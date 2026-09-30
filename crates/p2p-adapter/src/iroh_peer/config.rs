//! What legitimately varies between the nodes that run an iroh peer.

use std::sync::Arc;

use p2p::bitswap::AccessMode;
use p2p::iroh::IrohEndpointConfig;
use p2p::sync::SyncConfig;
use storage::stores::RetrySchedule;

use crate::ReplicatorPushOptionsState;

pub struct IrohPeerConfig {
    pub endpoint: IrohEndpointConfig,
    /// Bound before the peer handles any traffic: block serving, the
    /// coordinator, merge, outbound pushes, and pre-broadcast registration.
    pub document_acp: Arc<dyn acp::DocumentACP>,
    /// Register replicated documents' owners, which is only right under an
    /// authoritative shared ACP.
    pub strict_replicated_doc_access: bool,
    pub sync: SyncConfig,
    /// `Open` serves blocks to any peer; `Controlled` gates them on document
    /// ACP, which is right whenever a node has one.
    pub access_mode: AccessMode,
    /// Re-announce blocks merged from peers on their gossip topics.
    pub rebroadcast_on_merge: bool,
    pub max_merge_depth: usize,
    pub retry_schedule: RetrySchedule,
    /// Reload collection subscriptions persisted in the local store.
    pub load_persisted_collections: bool,
    /// Shared replicator push options, for callers that change them at runtime.
    pub replicator_push_options: Option<ReplicatorPushOptionsState>,
    /// App replication policy, installed before the peer handles traffic.
    pub replication_policy: Option<Arc<dyn p2p::replication_policy::ReplicationPolicy>>,
}

impl IrohPeerConfig {
    pub fn new(endpoint: IrohEndpointConfig, document_acp: Arc<dyn acp::DocumentACP>) -> Self {
        Self {
            endpoint,
            document_acp,
            strict_replicated_doc_access: false,
            sync: SyncConfig::default(),
            access_mode: AccessMode::Controlled,
            rebroadcast_on_merge: false,
            max_merge_depth: db::merge::DEFAULT_MAX_MERGE_DEPTH,
            retry_schedule: RetrySchedule::default(),
            load_persisted_collections: true,
            replicator_push_options: None,
            replication_policy: None,
        }
    }
}

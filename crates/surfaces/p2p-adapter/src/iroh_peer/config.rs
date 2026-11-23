//! What legitimately varies between the nodes that run an iroh peer.

use std::sync::Arc;

use edge::Edge;
use p2p::bitswap::AccessMode;
use p2p::iroh::{IrohEndpointConfig, StreamRegistration};
use p2p::sync::SyncConfig;
use storage::stores::RetrySchedule;

use crate::ReplicatorPushOptionsState;

pub struct IrohPeerConfig {
    pub endpoint: IrohEndpointConfig,
    /// The network runtime the peer's network work runs on.
    pub edge: Edge,
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
    /// The embedder's stream protocols, each with its handler.
    pub stream_protocols: Vec<StreamRegistration>,
}

impl IrohPeerConfig {
    pub fn new(
        endpoint: IrohEndpointConfig,
        document_acp: Arc<dyn acp::DocumentACP>,
        edge: Edge,
    ) -> Self {
        Self {
            endpoint,
            edge,
            document_acp,
            strict_replicated_doc_access: false,
            sync: SyncConfig::default(),
            access_mode: AccessMode::Controlled,
            rebroadcast_on_merge: false,
            max_merge_depth: db::merge::DEFAULT_MAX_MERGE_DEPTH,
            retry_schedule: RetrySchedule::default(),
            load_persisted_collections: true,
            replicator_push_options: None,
            stream_protocols: Vec::new(),
        }
    }
}

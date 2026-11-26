//! P2P initialization helpers for Node startup.

use std::sync::Arc;

use super::node::{Node, P2PTasks};
use crate::config::{AcpDocumentType, Config};
use crate::error::Result;

mod pinc;

type WireDocumentAcp = Option<Box<dyn FnOnce(Arc<dyn acp::DocumentACP>)>>;
type WireKms = Option<Box<dyn FnOnce(Arc<dyn kms::KmsService>) + Send>>;

pub(super) struct P2PSetup {
    pub(super) p2p_tasks: Option<P2PTasks>,
    pub(super) mutator: Arc<dyn query::mutator::DocMutator>,
    pub(super) http_adapter: Option<Arc<dyn defra_p2p_adapter::P2POperations>>,
    pub(super) wire_merge_acp: WireDocumentAcp,
    pub(super) wire_doc_pusher_acp: WireDocumentAcp,
    /// Hook for forwarding committed `/tx` writes to P2P peers. `Some` when the
    /// P2P stack is up; `None` for the non-P2P fallback path.
    pub(super) txn_broadcaster: Option<Arc<dyn db::event::emission::TxnBroadcaster>>,
    /// Type-erased KMS transport for this node's P2P system. server.rs adds it
    /// to the DefraKms transports list and installs the serve handler. `None`
    /// on the non-P2P fallback path.
    pub(super) kms_transport: Option<Arc<dyn kms::KeyTransport>>,
    /// Defers wiring the late-built KMS into the inner merge handler. NAC isn't
    /// available when the P2P system is created, so the KMS is built later in
    /// server.rs.
    pub(super) wire_kms: WireKms,
    /// This node's transport-level peer id (stringified). server.rs binds it
    /// into the KMS so served ECIES replies carry the correct AAD peer id.
    pub(super) local_peer_id: String,
    /// SE remote query transport (owner-queries-replicator, #976). `Some` when
    /// an SE key is present; `None` without one and on the non-P2P fallback.
    pub(super) se_transport: Option<Arc<dyn query::SeQueryTransport>>,
    /// Inbound management-channel serve deps, read lazily by the event loop and
    /// populated by server.rs once the controller (`P2POperations`) and NAC
    /// manager are built. `None` on the non-P2P fallback path; the event loop
    /// drops manage requests until populated.
    pub(super) manage_hooks: Option<defra_p2p_adapter::manage::hooks::ManageHooksCell>,
    /// The `P2POperations` controller (the `http_adapter`) bound into
    /// `manage_hooks` after the NAC manager exists. `None` on the fallback path.
    pub(super) manage_controller: Option<Arc<dyn defra_p2p_adapter::P2POperations>>,
    /// Requester-side manage correlators (mutating + query). Event-loop clones
    /// deliver inbound replies; these clones feed the requester API (Task 6.3)
    /// and are bound into `manage_hooks` so both agree on message_id
    /// correlation. `None` on the fallback path.
    pub(super) manage_correlator: Option<p2p::ManageCorrelator>,
    pub(super) manage_query_correlator: Option<p2p::ManageQueryCorrelator>,
    /// Outbound management requester (Task 7a): relays management requests to
    /// P2P-only peers on behalf of an HTTP caller. Built over the same concrete
    /// transport the SE requester / serve loop use, sharing the requester-side
    /// manage correlators. server.rs threads it into `AppState` via
    /// `with_manage`. `None` on the non-P2P fallback path.
    pub(super) manage_requester: Option<Arc<dyn defra_p2p_adapter::ManageRequester>>,
    /// Dials `net.peers` in the background. Called once node access control
    /// is wired, so no peer connects before it.
    pub(super) connect_bootstrap: Option<Box<dyn FnOnce() + Send>>,
}

impl Node {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn setup_p2p(
        store: Arc<storage::DynStore>,
        database: Arc<db::DB<storage::DynStore>>,
        event_bus: Arc<dyn events::Bus>,
        config: &Config,
        peer_seed: Option<zeroize::Zeroizing<[u8; 32]>>,
        node_identity: Option<Arc<identity::RawIdentity>>,
        se_key: Option<[u8; 32]>,
        document_acp: Arc<dyn acp::DocumentACP>,
        edge: &edge::Edge,
    ) -> Result<P2PSetup> {
        if config.net.p2p_disabled {
            return Ok(Self::p2p_disabled(database));
        }
        Self::setup_pinc_p2p(
            store,
            database,
            event_bus,
            config,
            peer_seed,
            node_identity,
            se_key,
            document_acp,
            edge,
        )
        .await
    }

    fn p2p_disabled(database: Arc<db::DB<storage::DynStore>>) -> P2PSetup {
        P2PSetup {
            p2p_tasks: None,
            mutator: Arc::new(db::AutoCommitMutator::new(database)),
            http_adapter: None,
            wire_merge_acp: None,
            wire_doc_pusher_acp: None,
            txn_broadcaster: None,
            kms_transport: None,
            wire_kms: None,
            local_peer_id: String::new(),
            se_transport: None,
            manage_hooks: None,
            manage_controller: None,
            manage_correlator: None,
            manage_query_correlator: None,
            manage_requester: None,
            connect_bootstrap: None,
        }
    }

    fn access_mode(config: &Config) -> p2p::access::AccessMode {
        if config.acp.document_type != AcpDocumentType::None {
            p2p::access::AccessMode::Controlled
        } else {
            p2p::access::AccessMode::Open
        }
    }

    fn sync_config(config: &Config) -> p2p::sync::SyncConfig {
        p2p::sync::SyncConfig {
            rate_limit_burst: config.net.p2p_rate_limit_burst,
            rate_limit_rate: config.net.p2p_rate_limit_rate,
            max_doc_sync_request_doc_ids: config.net.p2p_max_doc_sync_request_doc_ids,
            max_pending_dags: config.net.p2p_max_pending_dags,
            max_concurrent_dag_fetches: config.net.p2p_max_concurrent_dag_fetches,
            pending_dag_ttl: std::time::Duration::from_secs(config.net.p2p_pending_dag_ttl_secs),
            push_queue_capacity: config.net.p2p_push_queue_capacity,
            push_queue_byte_capacity: config.net.p2p_push_queue_byte_capacity,
            max_active_pushes_per_peer: config.net.p2p_max_active_pushes_per_peer,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_keeps_the_sync_defaults() {
        let sync = Node::sync_config(&Config::default());
        let defaults = p2p::sync::SyncConfig::default();
        assert_eq!(
            sync.max_concurrent_dag_fetches,
            defaults.max_concurrent_dag_fetches
        );
        assert_eq!(sync.pending_dag_ttl, defaults.pending_dag_ttl);
    }
}

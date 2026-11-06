//! Node state management for FFI.
//!
//! This module manages the lifecycle of node instances and their handles.
//! Go code receives opaque usize handles that map to actual node state.

mod handle;
mod identity;
mod p2p;
mod registry;
mod txn;

use std::sync::Arc;

use kovan::{Atom, AtomOption};
use storage::RegolithStore;
use zeroize::Zeroizing;

use blockstore::DefraBlockstore;

pub use identity::{IdentityHandle, IdentityRegistry, IdentityState, NodeIdentity, IDENTITIES};
pub use p2p::P2PState;
pub use registry::{GraphQLSubscriptionRegistry, NodeRegistry, GRAPHQL_SUBSCRIPTIONS, NODES};
pub use txn::{TxnHandle, TxnRegistry, TxnState, TXNS};

/// Type alias for the database type used in FFI.
pub type FfiDatabase = db::DB<RegolithStore>;

/// Type alias for the blockstore type used in FFI.
pub type FfiBlockstore = DefraBlockstore<RegolithStore>;

/// Type alias for the merge handler used in FFI.
pub type FfiMergeHandler = db::merge::DbMergeHandler<RegolithStore, FfiBlockstore>;

/// Type alias for node handles (opaque to FFI callers).
pub type NodeHandle = usize;

/// Type alias for the NAC manager used in FFI (dynamic dispatch over store backend).
pub type FfiNacManager = dyn db::NacManagerApi;

/// Type alias for subscription handles (opaque to FFI callers).
pub type SubscriptionHandle = usize;

/// State held for each FFI node.
pub struct NodeState {
    /// The node itself, held so its network scope and tasks outlive the
    /// handles copied out of it below.
    pub _node: defra_node::Node,
    /// The database instance.
    pub database: Arc<FfiDatabase>,
    /// Background tasks owned by the node (e.g. the downsample worker).
    pub background_tasks: Arc<defra_node::BackgroundTasks>,
    /// The query runner for executing GraphQL queries.
    pub query_runner: Arc<dyn query::QueryExecutor>,
    /// The NAC manager for node-level access control.
    pub nac_manager: Arc<FfiNacManager>,
    /// The document ACP for document-level access control.
    pub document_acp: Arc<dyn acp::DocumentACP>,
    /// The event bus for subscriptions.
    pub event_bus: Arc<dyn events::Bus>,
    /// Local Zanzibar policy store when document ACP is configured in local mode.
    pub local_zanzibar_store: Option<Arc<dyn acp::ZanzibarStore>>,
    /// P2P state (optional - not all nodes have P2P enabled).
    pub p2p: Option<Arc<P2PState>>,
    /// The node's identity (set when signing is enabled), with the node's
    /// hold on its key. Used as fallback identity for signing blocks when no
    /// explicit identity is provided.
    pub node_identity: Atom<Option<NodeIdentity>>,
    /// Whether block signing is enabled on this node.
    /// When true, anonymous requests still sign with node identity (matching Go).
    pub signing_enabled: bool,
    /// Query parsing and filter evaluation limits configured for this node.
    pub query_limits: query::QueryLimits,
    /// Searchable encryption key (32-byte AES-256 key). Zeroized on drop.
    pub se_encryption_key: AtomOption<Zeroizing<Vec<u8>>>,
    /// The node's options as `defra_node_options` reports them, redacted.
    pub options_json: serde_json::Value,
}

impl NodeState {
    /// The node's default signing identity DID, if one is configured.
    pub fn identity_did(&self) -> Option<String> {
        self.node_identity
            .peek(|identity| identity.as_ref().map(|identity| identity.did.clone()))
    }

    pub fn replicator_push_options(&self) -> defra_node::ReplicatorPushOptions {
        defra_node::ReplicatorPushOptions {
            se_encryption_key: self
                .se_encryption_key
                .load()
                .map(|key| Zeroizing::new(key.to_vec())),
            se_identity_pubkey: self.identity_did().map(String::into_bytes),
        }
    }

    pub fn sync_replicator_push_options(&self) -> Result<(), String> {
        let Some(p2p) = &self.p2p else {
            return Ok(());
        };
        p2p.system
            .set_replicator_push_options(self.replicator_push_options())
    }
}

/// State held for each GraphQL subscription (read by `defra_subscription_poll`).
///
/// Subscription queries are re-executed at **event time** (not poll time) to ensure
/// the DB state matches the event. A background network service processes events as
/// they arrive, executes the subscription query scoped to the changed document, and
/// buffers the full GraphQL JSON results for polling.
pub struct GraphQLSubscriptionState {
    /// Receiver for fully-processed GraphQL result JSON strings.
    pub result_receiver: kovan_channel::bounded::Receiver<String>,
    /// The node handle this subscription belongs to.
    pub node_handle: NodeHandle,
    /// The event bus subscription ID (for cleanup/unsubscribe).
    pub event_sub_id: u64,
    /// The background event processing service; stopping or dropping it
    /// stops the processing.
    pub service: edge::Service,
}

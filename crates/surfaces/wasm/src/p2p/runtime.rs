//! The browser's iroh peer: the shared peer every node runs, with a
//! relay-only endpoint and the browser's own document ACP. Encryption key
//! distribution is not wired here; a browser peer does not yet hold or serve
//! DEKs.

use std::sync::Arc;

use db::event::emission::TxnBroadcaster;
use db::DB;
use defra_p2p_adapter::{IrohPeer, IrohPeerConfig, IrohPeerShutdown, P2POperations};
use edge::{EdgeConfig, EdgeRuntime};
use p2p::iroh::IrohEndpointConfig;
use storage::RegolithStore;

use crate::error::{Result, WasmError};

use super::config::P2PConfig;

/// A running peer. Dropping it without [`P2PRuntime::shutdown`] leaves its
/// tasks running until the page unloads.
pub(crate) struct P2PRuntime {
    endpoint_id: String,
    shutdown: IrohPeerShutdown,
    /// The page's event loop, behind the network runtime handle the peer
    /// runs on. Started with P2P: a client without it starts none.
    edge: EdgeRuntime,
    pub(crate) ops: Arc<dyn P2POperations>,
    pub(crate) mutator: Arc<dyn query::DocMutator>,
    pub(crate) txn_broadcaster: Arc<dyn TxnBroadcaster>,
}

impl P2PRuntime {
    pub(crate) async fn start(
        database: Arc<DB<RegolithStore>>,
        event_bus: Arc<dyn events::Bus>,
        document_acp: Arc<dyn acp::DocumentACP>,
        identity: Option<Arc<identity::RawIdentity>>,
        config: &P2PConfig,
    ) -> Result<Self> {
        let endpoint = IrohEndpointConfig {
            secret_key: config.secret_key(identity.as_deref())?,
            node_identity: identity,
            relay_mode: config.relay_mode(),
            discovery: config.discovery(),
            ..IrohEndpointConfig::default()
        };
        let store = Arc::clone(database.store());
        let edge = EdgeRuntime::start(EdgeConfig::default())
            .map_err(|error| WasmError::P2P(format!("failed to start the peer: {error}")))?;
        // A browser's document ACP is its own, never an authoritative shared
        // one, so the config's non-strict default stands.
        let config = IrohPeerConfig::new(endpoint, document_acp, edge.edge().clone());
        let peer = IrohPeer::start(store, database, event_bus, config)
            .await
            .map_err(|error| WasmError::P2P(format!("failed to start the peer: {error}")))?;

        Ok(Self {
            endpoint_id: peer.local_peer_id.clone(),
            shutdown: peer.shutdown.clone(),
            edge,
            ops: Arc::clone(&peer.ops),
            mutator: peer.replication.broadcast_mutator.clone(),
            txn_broadcaster: Arc::clone(&peer.replication.txn_broadcaster),
        })
    }

    pub(crate) fn endpoint_id(&self) -> String {
        self.endpoint_id.clone()
    }

    pub(crate) async fn shutdown(self) {
        self.shutdown.shutdown().await;
        match self.edge.shutdown_async().await {
            Ok(drained) if drained.is_complete() => {}
            Ok(drained) => web_sys::console::warn_1(
                &format!(
                    "P2P stopped with {} network tasks still running",
                    drained.remaining
                )
                .into(),
            ),
            Err(error) => web_sys::console::warn_1(
                &format!("P2P network runtime did not shut down cleanly: {error}").into(),
            ),
        }
    }
}

//! The P2P system a node manages: its transport's shutdown, the replicator
//! options an application sets at run time, and the handles FFI and HTTP read.

use std::sync::Arc;

use defra_p2p_adapter::{ReplicatorPushOptions, ReplicatorPushOptionsState};

type ReplicatorPushOptionsCallback =
    Arc<dyn Fn(ReplicatorPushOptions) -> Result<(), String> + Send + Sync>;

/// On-demand replicator retry trigger. Runs a single retry pass that re-pushes
/// failed doc blocks AND regenerates/re-pushes their SE artifacts. Backs the
/// `defra_p2p_replicator_retry` FFI call, mirroring Go's `RetryReplicators`.
type RetryReplicatorsCallback =
    Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync>;

/// Runtime P2P transport kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportKind {
    #[cfg(feature = "p2p")]
    Pinc,
}

/// P2P runtime managed by a node.
pub struct ManagedP2PSystem {
    kind: TransportKind,
    ops: Arc<dyn defra_p2p_adapter::P2POperations>,
    replicator_push_options: ReplicatorPushOptionsState,
    on_replicator_push_options: Option<ReplicatorPushOptionsCallback>,
    retry_replicators: regolith::sync::OnceCell<RetryReplicatorsCallback>,
    /// Outbound management requester (Task 7a): relays management requests to
    /// P2P-only peers on behalf of an HTTP caller. Set once during P2P setup
    /// (after the system is built), consumed when wiring `AppState`.
    manage_requester: regolith::sync::OnceCell<Arc<dyn defra_p2p_adapter::ManageRequester>>,
    /// The pinc transport, for opening embedder stream protocols. Set once
    /// during P2P setup.
    #[cfg(feature = "p2p")]
    pinc_transport: regolith::sync::OnceCell<defra_pinc::PincTransport>,
    shutdown: ShutdownHandle,
}

impl ManagedP2PSystem {
    pub fn new(
        kind: TransportKind,
        ops: Arc<dyn defra_p2p_adapter::P2POperations>,
        shutdown: ShutdownHandle,
    ) -> Self {
        Self::with_replicator_push_options(
            kind,
            ops,
            shutdown,
            ReplicatorPushOptionsState::default(),
        )
    }

    pub fn with_replicator_push_options(
        kind: TransportKind,
        ops: Arc<dyn defra_p2p_adapter::P2POperations>,
        shutdown: ShutdownHandle,
        replicator_push_options: ReplicatorPushOptionsState,
    ) -> Self {
        Self::with_replicator_push_options_callback(
            kind,
            ops,
            shutdown,
            replicator_push_options,
            None,
        )
    }

    pub fn with_replicator_push_options_callback(
        kind: TransportKind,
        ops: Arc<dyn defra_p2p_adapter::P2POperations>,
        shutdown: ShutdownHandle,
        replicator_push_options: ReplicatorPushOptionsState,
        on_replicator_push_options: Option<ReplicatorPushOptionsCallback>,
    ) -> Self {
        Self {
            kind,
            ops,
            replicator_push_options,
            on_replicator_push_options,
            retry_replicators: regolith::sync::OnceCell::new(),
            manage_requester: regolith::sync::OnceCell::new(),
            #[cfg(feature = "p2p")]
            pinc_transport: regolith::sync::OnceCell::new(),
            shutdown,
        }
    }

    #[cfg(feature = "p2p")]
    pub(crate) fn set_pinc_transport(&self, transport: defra_pinc::PincTransport) {
        let _ = self.pinc_transport.set(transport);
    }

    #[cfg(feature = "p2p")]
    pub(crate) fn pinc_transport(&self) -> Option<&defra_pinc::PincTransport> {
        self.pinc_transport.get()
    }

    /// Install the outbound management requester. Set once during P2P setup.
    pub fn set_manage_requester(&self, requester: Arc<dyn defra_p2p_adapter::ManageRequester>) {
        let _ = self.manage_requester.set(requester);
    }

    /// Get the outbound management requester, if installed.
    pub fn manage_requester(&self) -> Option<&Arc<dyn defra_p2p_adapter::ManageRequester>> {
        self.manage_requester.get()
    }

    /// Install the on-demand replicator retry trigger. Set once during P2P setup.
    pub fn set_retry_replicators(&self, callback: RetryReplicatorsCallback) {
        let _ = self.retry_replicators.set(callback);
    }

    /// Run a single on-demand replicator retry pass (re-push failed doc blocks
    /// and regenerate/re-push their SE artifacts). Backs
    /// `defra_p2p_replicator_retry`.
    pub async fn retry_replicators(&self) -> Result<(), String> {
        match self.retry_replicators.get() {
            Some(callback) => {
                callback().await;
                Ok(())
            }
            None => Err("retry_replicators trigger not installed".to_string()),
        }
    }

    pub fn kind(&self) -> TransportKind {
        self.kind
    }

    pub fn ops(&self) -> &Arc<dyn defra_p2p_adapter::P2POperations> {
        &self.ops
    }

    /// The P2P operations serving the request `context` serves: their node
    /// access checks check the identity acting in it.
    pub fn ops_for(
        &self,
        context: &defra_core::RequestContext,
    ) -> Arc<dyn defra_p2p_adapter::P2POperations> {
        self.ops
            .bind_request(context)
            .unwrap_or_else(|| Arc::clone(&self.ops))
    }

    pub fn set_replicator_push_options(
        &self,
        options: ReplicatorPushOptions,
    ) -> Result<(), String> {
        self.replicator_push_options.store(options.clone())?;
        if let Some(callback) = &self.on_replicator_push_options {
            callback(options)?;
        }
        Ok(())
    }

    pub fn replicator_push_options(&self) -> ReplicatorPushOptions {
        self.replicator_push_options.load()
    }

    /// Stops the transport; [`Node::shutdown`](crate::Node::shutdown) calls it
    /// before closing the database.
    pub(crate) async fn stop_transport(&self) {
        self.shutdown.stop().await;
    }
}

pub struct ShutdownHandle {
    inner: ShutdownKind,
}

enum ShutdownKind {
    #[cfg(feature = "p2p")]
    Pinc {
        peer: defra_p2p_adapter::PincPeerShutdown,
        /// The peer's own scope of the network edge, drained after the peer
        /// stops: stopping it spawns the drain of its tasks, which the node's
        /// closed scope would refuse.
        scope: edge::Scope,
    },
}

impl ShutdownHandle {
    #[cfg(feature = "p2p")]
    pub(crate) fn pinc(peer: defra_p2p_adapter::PincPeerShutdown, scope: edge::Scope) -> Self {
        Self {
            inner: ShutdownKind::Pinc { peer, scope },
        }
    }

    /// Stops the transport and the tasks around it.
    pub async fn stop(&self) {
        #[cfg(not(feature = "p2p"))]
        match self.inner {}
        #[cfg(feature = "p2p")]
        match &self.inner {
            ShutdownKind::Pinc { peer, scope } => {
                peer.shutdown().await;
                crate::network::drain(
                    scope,
                    "timed out draining the node's network work during shutdown",
                )
                .await;
            }
        }
    }
}

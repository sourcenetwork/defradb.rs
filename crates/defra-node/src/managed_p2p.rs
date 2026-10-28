//! The P2P system a node manages: its transport's shutdown, the replicator
//! options an application sets at run time, and the handles FFI and HTTP read.

use std::sync::Arc;

#[cfg(feature = "libp2p")]
use kovan_queue::seg_queue::SegQueue;

use defra_p2p_adapter::{ReplicatorPushOptions, ReplicatorPushOptionsState};

type ReplicatorPushOptionsCallback =
    Arc<dyn Fn(ReplicatorPushOptions) -> Result<(), String> + Send + Sync>;

/// On-demand replicator retry trigger. Runs a single retry pass that re-pushes
/// failed doc blocks AND regenerates/re-pushes their SE artifacts. Backs the
/// `p2p_retry_replicators` FFI op, mirroring Go's `RetryReplicators`.
type RetryReplicatorsCallback =
    Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync>;

/// Runtime P2P transport kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportKind {
    #[cfg(feature = "libp2p")]
    Libp2p,
    #[cfg(feature = "p2p")]
    Iroh,
}

/// P2P runtime managed by an embedded node.
pub struct ManagedP2PSystem {
    kind: TransportKind,
    ops: Arc<dyn defra_http::P2POperations>,
    replicator_push_options: ReplicatorPushOptionsState,
    on_replicator_push_options: Option<ReplicatorPushOptionsCallback>,
    retry_replicators: regolith::sync::OnceCell<RetryReplicatorsCallback>,
    /// Outbound management requester (Task 7a): relays management requests to
    /// P2P-only peers on behalf of an HTTP caller. Set once during P2P setup
    /// (after the system is built), consumed when wiring `AppState`.
    manage_requester: regolith::sync::OnceCell<Arc<dyn defra_http::ManageRequester>>,
    shutdown: ShutdownHandle,
}

impl ManagedP2PSystem {
    pub fn new(
        kind: TransportKind,
        ops: Arc<dyn defra_http::P2POperations>,
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
        ops: Arc<dyn defra_http::P2POperations>,
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
        ops: Arc<dyn defra_http::P2POperations>,
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
            shutdown,
        }
    }

    /// Install the outbound management requester. Set once during P2P setup.
    pub fn set_manage_requester(&self, requester: Arc<dyn defra_http::ManageRequester>) {
        let _ = self.manage_requester.set(requester);
    }

    /// Get the outbound management requester, if installed.
    pub fn manage_requester(&self) -> Option<&Arc<dyn defra_http::ManageRequester>> {
        self.manage_requester.get()
    }

    /// Install the on-demand replicator retry trigger. Set once during P2P setup.
    pub fn set_retry_replicators(&self, callback: RetryReplicatorsCallback) {
        let _ = self.retry_replicators.set(callback);
    }

    /// Run a single on-demand replicator retry pass (re-push failed doc blocks
    /// and regenerate/re-push their SE artifacts). Backs `p2p_retry_replicators`.
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

    pub fn ops(&self) -> &Arc<dyn defra_http::P2POperations> {
        &self.ops
    }

    /// The P2P operations serving the request `context` serves: their node
    /// access checks check the identity acting in it.
    pub fn ops_for(
        &self,
        context: &defra_core::RequestContext,
    ) -> Arc<dyn defra_http::P2POperations> {
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

    /// Stops the transport, then clears the process's signing identities, as
    /// the FFI node's close does.
    pub async fn shutdown(&self) {
        self.shutdown.stop().await;
        defra_core::signing::clear_identity_store();
    }

    /// Stops the transport and leaves the signing identities alone.
    #[cfg(any(feature = "libp2p", feature = "p2p"))]
    pub(crate) async fn stop_transport(&self) {
        self.shutdown.stop().await;
    }
}

pub struct ShutdownHandle {
    inner: ShutdownKind,
}

enum ShutdownKind {
    #[cfg(feature = "libp2p")]
    Libp2p {
        handle: Box<p2p::P2PHostHandle>,
        coordinator: p2p::sync::SyncShutdownHandle,
        tasks: Box<SegQueue<edge::Service>>,
        /// The retry sweep, on the network edge.
        retry_loop: Box<SegQueue<edge::Task<()>>>,
        /// The push failure recorder, on the worker pool.
        recorder: Box<SegQueue<workers::Background>>,
    },
    #[cfg(feature = "p2p")]
    Iroh(defra_p2p_adapter::IrohPeerShutdown),
}

impl ShutdownHandle {
    #[cfg(feature = "libp2p")]
    pub(crate) fn libp2p(
        handle: p2p::P2PHostHandle,
        coordinator: p2p::sync::SyncShutdownHandle,
        tasks: Vec<edge::Service>,
        retry_loop: edge::Task<()>,
        recorder: workers::Background,
    ) -> Self {
        Self {
            inner: ShutdownKind::Libp2p {
                handle: Box::new(handle),
                coordinator,
                tasks: queued(tasks),
                retry_loop: queued([retry_loop]),
                recorder: queued([recorder]),
            },
        }
    }

    #[cfg(feature = "p2p")]
    pub(crate) fn iroh(peer: defra_p2p_adapter::IrohPeerShutdown) -> Self {
        Self {
            inner: ShutdownKind::Iroh(peer),
        }
    }

    /// Stops the transport and the tasks around it.
    pub async fn stop(&self) {
        #[cfg(not(any(feature = "libp2p", feature = "p2p")))]
        match self.inner {}
        #[cfg(any(feature = "libp2p", feature = "p2p"))]
        match &self.inner {
            #[cfg(feature = "libp2p")]
            ShutdownKind::Libp2p {
                handle,
                coordinator,
                tasks,
                retry_loop,
                recorder,
            } => {
                coordinator.shutdown().await;
                abort_and_join(retry_loop, edge::Task::abort).await;
                abort_and_join(recorder, workers::Background::abort).await;
                stop_services(tasks).await;
                let _ = handle.shutdown().await;
            }
            #[cfg(feature = "p2p")]
            ShutdownKind::Iroh(peer) => peer.shutdown().await,
        }
    }
}

/// `tasks` in a queue a shutdown through `&self` can take them from.
#[cfg(feature = "libp2p")]
fn queued<T>(tasks: impl IntoIterator<Item = T>) -> Box<SegQueue<T>>
where
    T: 'static,
{
    let pending = SegQueue::new();
    tasks.into_iter().for_each(|task| pending.push(task));
    Box::new(pending)
}

#[cfg(feature = "libp2p")]
async fn stop_services(services: &SegQueue<edge::Service>) {
    let mut stopping = Vec::with_capacity(services.len());
    while let Some(service) = services.pop() {
        service.stop();
        stopping.push(service);
    }
    for service in stopping {
        service.stopped().await;
    }
}

#[cfg(feature = "libp2p")]
async fn abort_and_join<T, A>(tasks: &SegQueue<T>, abort: A)
where
    T: std::future::Future + 'static,
    A: Fn(&T),
{
    let mut aborted = Vec::with_capacity(tasks.len());
    while let Some(task) = tasks.pop() {
        abort(&task);
        aborted.push(task);
    }
    for task in aborted {
        let _ = task.await;
    }
}

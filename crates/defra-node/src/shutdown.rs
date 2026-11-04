//! Owned, cancellation-resilient node teardown.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::network::NodeEdge;
#[cfg(not(target_arch = "wasm32"))]
use crate::BackgroundTasks;
#[cfg(feature = "managed-p2p")]
use crate::ManagedP2PSystem;
#[cfg(feature = "otel")]
use crate::TelemetryHandle;
use crate::{Error, Result};

pub(crate) struct Teardown {
    database: Arc<db::DB<storage::RegolithStore>>,
    txn_registry: Arc<db::DbTransactionRegistry<storage::RegolithStore>>,
    network: Option<Arc<NodeEdge>>,
    #[cfg(feature = "http")]
    txn_cleanup_task: kovan_queue::seg_queue::SegQueue<workers::Background>,
    #[cfg(not(target_arch = "wasm32"))]
    background_tasks: Arc<BackgroundTasks>,
    #[cfg(feature = "managed-p2p")]
    p2p: Option<Arc<ManagedP2PSystem>>,
    #[cfg(feature = "otel")]
    telemetry: kovan_queue::seg_queue::SegQueue<TelemetryHandle>,
    initiated: AtomicBool,
    started: AtomicBool,
    finished: AtomicBool,
    interrupted: AtomicBool,
    done: regolith::sync::Notify,
    close_result: Mutex<Option<std::result::Result<(), Arc<db::Error>>>>,
    #[cfg(test)]
    test_gate: Mutex<Option<Arc<tests::Gate>>>,
    #[cfg(test)]
    test_panic: AtomicBool,
    #[cfg(test)]
    test_fail_close: AtomicBool,
}

impl Teardown {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        database: Arc<db::DB<storage::RegolithStore>>,
        txn_registry: Arc<db::DbTransactionRegistry<storage::RegolithStore>>,
        network: Option<Arc<NodeEdge>>,
        #[cfg(feature = "http")] txn_cleanup_task: Option<workers::Background>,
        #[cfg(not(target_arch = "wasm32"))] background_tasks: Arc<BackgroundTasks>,
        #[cfg(feature = "managed-p2p")] p2p: Option<Arc<ManagedP2PSystem>>,
        #[cfg(feature = "otel")] telemetry: Option<TelemetryHandle>,
    ) -> Self {
        Self {
            database,
            txn_registry,
            network,
            #[cfg(feature = "http")]
            txn_cleanup_task: crate::single_slot(txn_cleanup_task),
            #[cfg(not(target_arch = "wasm32"))]
            background_tasks,
            #[cfg(feature = "managed-p2p")]
            p2p,
            #[cfg(feature = "otel")]
            telemetry: crate::single_slot(telemetry),
            initiated: AtomicBool::new(false),
            started: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            interrupted: AtomicBool::new(false),
            done: regolith::sync::Notify::new(),
            close_result: Mutex::new(None),
            #[cfg(test)]
            test_gate: Mutex::new(None),
            #[cfg(test)]
            test_panic: AtomicBool::new(false),
            #[cfg(test)]
            test_fail_close: AtomicBool::new(false),
        }
    }

    pub(crate) fn is_shutdown(&self) -> bool {
        self.initiated.load(Ordering::SeqCst)
    }

    pub(crate) async fn shutdown(self: &Arc<Self>) -> Result<()> {
        // Native shutdown lives on the process-wide runtime, never on a
        // caller or on the runtime the node is about to stop. On wasm the
        // page's event loop drives a separate owned runtime scope.
        #[cfg(not(target_arch = "wasm32"))]
        let runtime = edge::on_demand().map_err(Error::ShutdownOwner)?.clone();
        #[cfg(target_arch = "wasm32")]
        let mut owner = Some(
            edge::EdgeRuntime::start(edge::EdgeConfig::default()).map_err(Error::ShutdownOwner)?,
        );
        #[cfg(target_arch = "wasm32")]
        let runtime = owner
            .as_ref()
            .expect("owner was just started")
            .edge()
            .clone();
        self.initiated.store(true, Ordering::SeqCst);
        loop {
            let notified = self.done.notified();
            if self.finished.load(Ordering::SeqCst) {
                break;
            }
            if !self.started.swap(true, Ordering::SeqCst) {
                let leading = Leading(Arc::clone(self));
                // Dropping this task handle detaches the work. A refused or
                // panicking owner drops `leading` and records failure; it is
                // not safe to replay a partially completed transport or close.
                #[cfg(target_arch = "wasm32")]
                let task_owner = owner.take().expect("only the leader starts teardown");
                let task = runtime
                    .spawn_pinned(move || async move {
                        #[cfg(target_arch = "wasm32")]
                        let _owner = task_owner;
                        leading.0.run().await;
                        drop(leading);
                    })
                    .within(Duration::MAX);
                drop(task);
            }
            notified.await;
        }
        if self.interrupted.load(Ordering::SeqCst) {
            return Err(Error::ShutdownInterrupted);
        }
        match self
            .close_result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            Some(Ok(())) => Ok(()),
            Some(Err(error)) => Err(Error::Shutdown(Arc::clone(error))),
            None => unreachable!("finished teardown must have a close result"),
        }
    }

    async fn run(&self) {
        #[cfg(test)]
        {
            let gate = self.test_gate.lock().unwrap().clone();
            if let Some(gate) = gate {
                gate.signal(&gate.entered);
                gate.wait_for(&gate.released).await;
            }
        }
        #[cfg(test)]
        assert!(
            !self.test_panic.load(Ordering::SeqCst),
            "forced shutdown owner panic"
        );
        tracing::info!("node shutdown: begin");

        #[cfg(feature = "http")]
        if let Some(task) = self.txn_cleanup_task.pop() {
            task.abort();
            let _ = workers::timeout(Duration::from_secs(1), task).await;
        }

        #[cfg(not(target_arch = "wasm32"))]
        self.background_tasks.shutdown().await;

        // Requests first: no new signed query, in-process query, transaction
        // commit or HTTP request is admitted, and those in flight finish while
        // P2P still records their writes' durable head markers.
        if let Some(network) = &self.network {
            network.drain_requests().await;
        }

        // The transport and the tasks around it stop next and are awaited, so
        // none still reaches storage when the drain and the close come.
        #[cfg(feature = "managed-p2p")]
        if let Some(p2p) = &self.p2p {
            p2p.stop_transport().await;
        }

        // Stop accepting the rest of the node's network work and drain what is
        // in flight.
        if let Some(network) = &self.network {
            network.drain().await;
        }

        // Nothing opens a transaction for the node any more: those left open
        // (a client gone mid-transaction, a commit refused above) end while
        // their owners' workers still run, so the store's close does not
        // wait out its timeout for them.
        match self
            .txn_registry
            .cleanup_stale_transactions(Duration::ZERO)
            .await
        {
            Ok(ended) if ended.cleaned > 0 => {
                tracing::info!(
                    ended = ended.cleaned,
                    "shutdown ended transactions left open"
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "shutdown could not end the transactions left open");
            }
        }

        // Nothing reaches the data path any more: the worker pool finishes
        // what it admitted and stops, then the store closes.
        // Retain the exact failure so all shutdown callers receive it.
        let already_closed = self
            .close_result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some();
        if !already_closed {
            let closed = self.database.close().await.map_err(Arc::new);
            #[cfg(test)]
            let closed = if self.test_fail_close.load(Ordering::SeqCst) && closed.is_ok() {
                Err(Arc::new(db::Error::Other("forced close failure".into())))
            } else {
                closed
            };
            *self.close_result.lock().unwrap_or_else(|e| e.into_inner()) = Some(closed);
        }

        // Flush buffered spans / metrics. Done after P2P shutdown so trailing
        // shutdown spans are still captured. Go DefraDB never calls provider
        // shutdown — we don't repeat that bug.
        //
        // `handle.shutdown()` synchronously joins the SDK batch-exporter
        // thread (up to ~5 s, double with metrics), so run it on a blocking
        // thread of the network runtime rather than stalling the caller.
        #[cfg(feature = "otel")]
        if let Some(handle) = self.telemetry.pop() {
            let runtime = self
                .network
                .as_ref()
                .map(|network| Ok(network.runtime().clone()))
                .unwrap_or_else(|| edge::on_demand().cloned());
            // The slot keeps the handle when the runtime refuses the work,
            // so its flush moves to a thread of its own instead of running
            // on the caller as the dropped closure's handle.
            let slot = Arc::new(crate::single_slot(Some(handle)));
            let flushing = Arc::clone(&slot);
            let flushed = match runtime {
                Ok(runtime) => runtime
                    .spawn_blocking(move || {
                        if let Some(handle) = flushing.pop() {
                            handle.shutdown();
                        }
                    })
                    .await
                    .map_err(|error| error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            if let Err(error) = flushed {
                tracing::warn!(%error, "telemetry did not flush on the network runtime");
            }
            if let Some(handle) = slot.pop() {
                if let Err(error) = std::thread::Builder::new()
                    .name("defra-otel-shutdown".to_string())
                    .spawn(move || handle.shutdown())
                {
                    tracing::warn!(%error, "telemetry could not start its flush thread");
                }
            }
        }

        // This task is independent of the node runtime, so it can await
        // that runtime's stop before releasing any shutdown callers.
        if let Some(network) = &self.network {
            network.stop_owned().await;
        }
        self.finished.store(true, Ordering::SeqCst);
        self.done.notify_waiters();
        tracing::info!("node shutdown: complete");
    }
    fn release_runtime(&self) {
        if let Some(network) = &self.network {
            network.release_owned();
        }
    }
}

struct Leading(Arc<Teardown>);
impl Drop for Leading {
    fn drop(&mut self) {
        if !self.0.finished.load(Ordering::SeqCst) {
            self.0.interrupted.store(true, Ordering::SeqCst);
            self.0.finished.store(true, Ordering::SeqCst);
            self.0.done.notify_waiters();
            self.0.release_runtime();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Node;

    #[derive(Default)]
    pub(super) struct Gate {
        pub(super) entered: AtomicBool,
        pub(super) released: AtomicBool,
        changed: regolith::sync::Notify,
    }

    impl Gate {
        pub(super) async fn wait_for(&self, flag: &AtomicBool) {
            loop {
                let notified = self.changed.notified();
                if flag.load(Ordering::SeqCst) {
                    return;
                }
                notified.await;
            }
        }
        pub(super) fn signal(&self, flag: &AtomicBool) {
            flag.store(true, Ordering::SeqCst);
            self.changed.notify_waiters();
        }
    }

    async fn exercise_cancelled_leader(fail_close: bool) {
        let node = Arc::new(Node::builder().build().await.unwrap());
        let gate = Arc::new(Gate::default());
        *node.teardown.test_gate.lock().unwrap() = Some(gate.clone());
        node.teardown
            .test_fail_close
            .store(fail_close, Ordering::SeqCst);
        let runtime = edge::testing::edge().clone();
        let leader = runtime.spawn({
            let node = node.clone();
            async move { node.shutdown().await }
        });
        runtime
            .timeout(Duration::from_secs(20), gate.wait_for(&gate.entered))
            .await
            .unwrap();
        leader.abort();
        let _ = leader.await;

        let first = runtime.spawn({
            let node = node.clone();
            async move { node.shutdown().await }
        });
        let second = runtime.spawn({
            let node = node.clone();
            async move { node.shutdown().await }
        });
        assert!(!first.is_finished());
        assert!(!second.is_finished());
        gate.signal(&gate.released);
        let (first, second) = futures::join!(
            first.within(Duration::from_secs(20)),
            second.within(Duration::from_secs(20))
        );
        let first = first.unwrap();
        let second = second.unwrap();
        if fail_close {
            assert!(first
                .unwrap_err()
                .to_string()
                .contains("forced close failure"));
            assert!(second
                .unwrap_err()
                .to_string()
                .contains("forced close failure"));
            assert!(node
                .shutdown()
                .await
                .unwrap_err()
                .to_string()
                .contains("forced close failure"));
        } else {
            first.unwrap();
            second.unwrap();
            node.shutdown().await.unwrap();
        }
    }

    #[test]
    fn cancelled_leader_leaves_owned_teardown_running_and_shares_close_error() {
        edge::testing::edge()
            .block_on(exercise_cancelled_leader(true))
            .unwrap();
    }

    #[test]
    fn refused_owner_reports_failure_without_running_teardown() {
        edge::testing::edge()
            .block_on(async {
                let node = Arc::new(Node::builder().build().await.unwrap());
                let scope = edge::testing::edge().scope();
                let _ = scope.drain(Duration::ZERO).await;
                node.teardown.initiated.store(true, Ordering::SeqCst);
                node.teardown.started.store(true, Ordering::SeqCst);
                let leading = Leading(Arc::clone(&node.teardown));
                let task = scope.edge().spawn_pinned(move || async move {
                    leading.0.run().await;
                    drop(leading);
                });
                drop(task);
                assert!(matches!(
                    node.shutdown().await,
                    Err(Error::ShutdownInterrupted)
                ));
                assert!(matches!(
                    node.shutdown().await,
                    Err(Error::ShutdownInterrupted)
                ));
            })
            .unwrap();
    }

    #[test]
    fn panicked_owner_reports_failure_to_all_callers_without_retry() {
        edge::testing::edge()
            .block_on(async {
                let node = Arc::new(Node::builder().build().await.unwrap());
                let gate = Arc::new(Gate::default());
                *node.teardown.test_gate.lock().unwrap() = Some(gate.clone());
                node.teardown.test_panic.store(true, Ordering::SeqCst);
                let runtime = edge::testing::edge().clone();
                let first = runtime.spawn({
                    let node = node.clone();
                    async move { node.shutdown().await }
                });
                runtime
                    .timeout(Duration::from_secs(20), gate.wait_for(&gate.entered))
                    .await
                    .unwrap();
                let second = runtime.spawn({
                    let node = node.clone();
                    async move { node.shutdown().await }
                });
                gate.signal(&gate.released);
                let (first, second) = futures::join!(
                    first.within(Duration::from_secs(20)),
                    second.within(Duration::from_secs(20))
                );
                assert!(matches!(first.unwrap(), Err(Error::ShutdownInterrupted)));
                assert!(matches!(second.unwrap(), Err(Error::ShutdownInterrupted)));
                assert!(matches!(
                    node.shutdown().await,
                    Err(Error::ShutdownInterrupted)
                ));
            })
            .unwrap();
    }
}

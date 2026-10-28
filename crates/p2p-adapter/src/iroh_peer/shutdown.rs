//! Stopping a peer, in one order for every node.

use std::sync::Arc;
use std::time::Duration;

use kovan_queue::seg_queue::SegQueue;
use n0_future::task::JoinHandle;
use p2p::iroh::IrohTransport;
use p2p::P2PTransport;

const TASK_STOP_TIMEOUT: Duration = Duration::from_secs(1);
const ENDPOINT_STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Stops the peer once; later calls, from any clone, return immediately.
#[derive(Clone)]
pub struct IrohPeerShutdown {
    /// One slot: popping hands back an owned `Parts`, so every handle is
    /// stopped here instead of at a deferred drop.
    parts: Arc<SegQueue<Parts>>,
}

struct Parts {
    transport: IrohTransport,
    coordinator: p2p::sync::SyncShutdownHandle,
    endpoint_task: JoinHandle<()>,
    retry_loop_task: edge::Task<()>,
    tasks: Vec<JoinHandle<()>>,
    failure_recorder_task: crate::FailureRecorder,
    edge: edge::Edge,
}

impl IrohPeerShutdown {
    pub(super) fn new(
        transport: IrohTransport,
        coordinator: p2p::sync::SyncShutdownHandle,
        endpoint_task: JoinHandle<()>,
        retry_loop_task: edge::Task<()>,
        tasks: Vec<JoinHandle<()>>,
        failure_recorder_task: crate::FailureRecorder,
        edge: edge::Edge,
    ) -> Self {
        let parts = SegQueue::new();
        parts.push(Parts {
            transport,
            coordinator,
            endpoint_task,
            retry_loop_task,
            tasks,
            failure_recorder_task,
            edge,
        });
        Self {
            parts: Arc::new(parts),
        }
    }

    /// No new retries, then the coordinator's own work, then the endpoint,
    /// then the tasks that only consumed what those produced.
    pub async fn shutdown(&self) {
        let Some(Parts {
            transport,
            coordinator,
            mut endpoint_task,
            retry_loop_task,
            tasks,
            failure_recorder_task,
            edge,
        }) = self.parts.pop()
        else {
            return;
        };

        stop_task(&edge, retry_loop_task, edge::Task::abort).await;
        coordinator.shutdown().await;
        if let Err(error) = transport.shutdown().await {
            tracing::debug!(%error, "iroh transport shutdown returned an error");
        }
        drop(transport);

        for task in tasks {
            stop_task(&edge, task, JoinHandle::abort).await;
        }
        stop_task(&edge, failure_recorder_task, crate::FailureRecorder::abort).await;

        if edge
            .timeout(ENDPOINT_STOP_TIMEOUT, &mut endpoint_task)
            .await
            .is_err()
        {
            tracing::warn!("iroh endpoint did not stop after graceful shutdown; aborting");
            stop_task(&edge, endpoint_task, JoinHandle::abort).await;
        }
    }
}

async fn stop_task<T, A>(edge: &edge::Edge, task: T, abort: A)
where
    T: std::future::Future,
    A: FnOnce(&T),
{
    abort(&task);
    let _ = edge.timeout(TASK_STOP_TIMEOUT, task).await;
}

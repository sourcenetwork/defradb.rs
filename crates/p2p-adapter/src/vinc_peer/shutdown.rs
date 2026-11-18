//! Stopping a peer, in one order for every node.

use std::sync::Arc;
use std::time::Duration;

use kovan_queue::seg_queue::SegQueue;
use p2p::vinc::VincTransport;
use p2p::P2PTransport;

const TASK_STOP_TIMEOUT: Duration = Duration::from_secs(1);
const ENDPOINT_STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Stops the peer once; later calls, from any clone, return immediately.
#[derive(Clone)]
pub struct VincPeerShutdown {
    /// One slot: popping hands back an owned `Parts`, so every handle is
    /// stopped here instead of at a deferred drop.
    parts: Arc<SegQueue<Parts>>,
}

struct Parts {
    transport: VincTransport,
    coordinator: p2p::sync::SyncShutdownHandle,
    endpoint_task: edge::Task<()>,
    retry_loop_task: edge::Task<()>,
    tasks: Vec<edge::Service>,
    failure_recorder_task: crate::FailureRecorder,
    edge: edge::Edge,
}

impl VincPeerShutdown {
    pub(super) fn new(
        transport: VincTransport,
        coordinator: p2p::sync::SyncShutdownHandle,
        endpoint_task: edge::Task<()>,
        retry_loop_task: edge::Task<()>,
        tasks: Vec<edge::Service>,
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
            endpoint_task,
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
            tracing::debug!(%error, "vinc transport shutdown returned an error");
        }
        drop(transport);

        for task in tasks {
            let _ = edge.timeout(TASK_STOP_TIMEOUT, task.stopped()).await;
        }
        stop_task(&edge, failure_recorder_task, crate::FailureRecorder::abort).await;

        let mut endpoint = std::pin::pin!(endpoint_task);
        if edge
            .timeout(ENDPOINT_STOP_TIMEOUT, endpoint.as_mut())
            .await
            .is_err()
        {
            tracing::warn!("vinc endpoint did not stop after graceful shutdown; aborting");
            endpoint.abort();
            let _ = edge.timeout(TASK_STOP_TIMEOUT, endpoint).await;
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

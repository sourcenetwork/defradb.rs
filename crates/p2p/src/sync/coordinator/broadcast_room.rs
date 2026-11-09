//! The byte budget for committed writes' gossip waiting to be published
//! (#2489): blocks in the coalescing window or waiting for a publish slot.
//! Work with room runs on one lane task, so however many writes wait, the
//! coordinator retains a fixed number of task handles (#1099).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use defra_core::thread_bounds::MaybeBoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use futures::FutureExt;
use kovan_queue::seg_queue::SegQueue;
use regolith::sync::{OwnedSemaphorePermit, Semaphore, UnboundedReceiver, UnboundedSender};

use super::SyncShutdownHandle;
use crate::network::Network;

/// One committed write's gossip, holding its room until it finishes. A named
/// type, so the channel's `T: 'static` bound holds under the lane's `Send`
/// check, which a bare boxed `dyn Future` does not prove.
struct Work(MaybeBoxFuture<'static, ()>);

impl Future for Work {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.0.as_mut().poll(cx)
    }
}

pub(crate) struct BroadcastRoom {
    bytes: usize,
    room: Arc<Semaphore>,
    /// Work with room, for the lane.
    work: UnboundedSender<Work>,
    /// The lane's end of `work` until the lane starts.
    idle: SegQueue<UnboundedReceiver<Work>>,
}

impl BroadcastRoom {
    /// A budget of `bytes`, at least one.
    pub(crate) fn new(bytes: usize) -> Self {
        let bytes = bytes.clamp(1, Semaphore::MAX_PERMITS);
        let (work, lane) = regolith::sync::unbounded();
        let idle = SegQueue::new();
        idle.push(lane);
        Self {
            bytes,
            room: Arc::new(Semaphore::new(bytes)),
            work,
            idle,
        }
    }

    /// Room for `bytes`, waiting while the budget is full; `None` once
    /// `deadline` passes first. Work larger than the whole budget takes all
    /// of it, so it still runs alone.
    pub(crate) async fn reserve(
        &self,
        network: &Network,
        bytes: usize,
        deadline: Duration,
    ) -> Option<OwnedSemaphorePermit> {
        let bytes = bytes.clamp(1, self.bytes);
        if let Some(permit) = self.room.acquire_owned(bytes).now_or_never() {
            return Some(permit);
        }
        network
            .timeout(deadline, self.room.acquire_owned(bytes))
            .await
            .ok()
    }

    /// Starts the lane, once; later calls change nothing.
    pub(crate) fn start(&self, shutdown: &SyncShutdownHandle) {
        if let Some(queued) = self.idle.pop() {
            shutdown.spawn_task(lane(queued, shutdown.clone()));
        }
    }

    /// Runs `work`, which already holds its room, on the lane: one
    /// coordinator task polling every admitted write's gossip at once. A
    /// lane not started yet starts here.
    pub(crate) fn run(&self, shutdown: &SyncShutdownHandle, work: MaybeBoxFuture<'static, ()>) {
        self.start(shutdown);
        let _ = self.work.try_send(Work(work));
    }

    /// Bytes reserved right now.
    pub(crate) fn in_use(&self) -> usize {
        self.bytes - self.room.available_permits()
    }
}

/// Polls the admitted gossip until shutdown, then lets what it holds finish
/// within the shutdown drain's budget.
async fn lane(queued: UnboundedReceiver<Work>, shutdown: SyncShutdownHandle) {
    // Boxed once: its `Send` is proved here, not under the select's borrows.
    let mut stopping = Box::pin(async move { shutdown.cancelled().await }).fuse();
    let mut running = FuturesUnordered::new();
    loop {
        if running.is_empty() {
            // An empty FuturesUnordered yields None immediately; only wait on
            // the queue (or shutdown) until there is work to poll again.
            futures::select! {
                work = queued.recv_async().fuse() => match work {
                    Some(work) => running.push(work),
                    None => break,
                },
                () = &mut stopping => break,
            }
            continue;
        }
        futures::select! {
            work = queued.recv_async().fuse() => match work {
                Some(work) => running.push(work),
                None => break,
            },
            _done = running.next().fuse() => {}
            () = &mut stopping => break,
        }
    }
    while running.next().await.is_some() {}
}

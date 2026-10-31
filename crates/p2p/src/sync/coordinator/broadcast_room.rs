//! The byte budget for committed writes' gossip waiting to be published
//! (#2489): blocks in the coalescing window or waiting for a publish slot.

use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use regolith::sync::{OwnedSemaphorePermit, Semaphore};

use crate::network::Network;

pub(crate) struct BroadcastRoom {
    bytes: usize,
    room: Arc<Semaphore>,
}

impl BroadcastRoom {
    /// A budget of `bytes`, at least one.
    pub(crate) fn new(bytes: usize) -> Self {
        let bytes = bytes.clamp(1, Semaphore::MAX_PERMITS);
        Self {
            bytes,
            room: Arc::new(Semaphore::new(bytes)),
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

    /// Bytes reserved right now.
    pub(crate) fn in_use(&self) -> usize {
        self.bytes - self.room.available_permits()
    }
}

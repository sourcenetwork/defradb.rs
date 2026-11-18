//! The one background reconciler of a collection topic.
//!
//! A reconciler converges one collection's live topic to its desired state:
//! it reads the desired state, installs or removes the topic, and reads the
//! desired state again, and it ends once the two agree. A collection has at
//! most one reconciler. A change that finds one running leaves the topic to
//! it, so a reconciler that ends looks once more after it lets go, and runs
//! on when that look finds the topic apart and nobody took over.

use std::sync::Arc;
use std::time::Duration;

use super::{Shared, TopicChange};
use crate::transport::P2PTransport;
use futures::FutureExt;

const RECONCILE_RETRY_MIN: Duration = Duration::from_millis(100);
const RECONCILE_RETRY_MAX: Duration = Duration::from_secs(5);

/// How one pass of a reconciler ended.
enum Pass {
    /// The live topic agrees with the desired state.
    Converged,
    /// The desired state changed while the topic was being changed.
    Changed,
    /// A durable read or the transport failed.
    Failed,
}

impl<T> Shared<T>
where
    T: P2PTransport,
{
    /// Starts the reconciler of a collection, unless one runs already.
    pub(super) fn reconcile(self: &Arc<Self>, collection_id: String) {
        if self
            .reconciling
            .insert_if_absent(collection_id.clone(), ())
            .is_some()
        {
            return;
        }
        let shared = Arc::clone(self);
        let task_collection_id = collection_id.clone();
        if !self
            .shutdown
            .spawn_task(async move { shared.run(task_collection_id).await })
        {
            self.reconciling.remove(&collection_id);
        }
    }

    async fn run(self: Arc<Self>, collection_id: String) {
        let mut delay = RECONCILE_RETRY_MIN;
        loop {
            futures::select! {
                _ = self.shutdown.cancelled().fuse() => break,
                _ = crate::rt::time::sleep(delay).fuse() => {}
            }
            match self.pass(&collection_id).await {
                Pass::Converged => {
                    self.reconciling.remove(&collection_id);
                    if self.converged(&collection_id).await
                        || self
                            .reconciling
                            .insert_if_absent(collection_id.clone(), ())
                            .is_some()
                    {
                        return;
                    }
                    delay = RECONCILE_RETRY_MIN;
                }
                Pass::Changed => delay = RECONCILE_RETRY_MIN,
                Pass::Failed => delay = (delay * 2).min(RECONCILE_RETRY_MAX),
            }
        }
        self.reconciling.remove(&collection_id);
    }

    /// Reads the desired state, moves the live topic to it, and reads the
    /// desired state again. A topic to remove is removed even when the live
    /// set does not hold it: a subscribe that timed out may have joined it.
    async fn pass(&self, collection_id: &str) -> Pass {
        let Some(desired) = self.desired(collection_id).await else {
            return Pass::Failed;
        };
        if !(desired && self.live.contains_key(collection_id)) {
            if let Err(error) = self
                .change_topic(collection_id, TopicChange::towards(desired))
                .await
            {
                tracing::warn!(
                    collection_id = %collection_id,
                    error = %error,
                    "Collection topic change failed; retrying in the background"
                );
                return Pass::Failed;
            }
        }
        match self.desired(collection_id).await {
            Some(now) if now == desired => Pass::Converged,
            Some(_) => Pass::Changed,
            None => Pass::Failed,
        }
    }
}

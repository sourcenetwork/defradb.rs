use std::sync::Arc;

use crate::{ReplicatorInstall, TransportDocPusher};

/// Reports cancellation as an install failure without holding a storage guard.
pub(crate) struct ReplayOwner {
    pusher: Arc<dyn TransportDocPusher>,
    bus: Option<Arc<dyn events::Bus>>,
    pub(crate) plan: Option<ReplicatorInstall>,
}

impl ReplayOwner {
    pub(crate) fn new(
        pusher: Arc<dyn TransportDocPusher>,
        plan: ReplicatorInstall,
        bus: Option<Arc<dyn events::Bus>>,
    ) -> Self {
        Self {
            pusher,
            bus,
            plan: Some(plan),
        }
    }

    pub(crate) async fn complete(&mut self, error: Option<String>) {
        if let Some(plan) = &self.plan {
            if let Err(error) = self
                .pusher
                .finish_replicator_install(plan, error, self.bus.clone())
                .await
            {
                tracing::error!(%error, "failed to persist replicator completion");
            }
        }
        self.plan = None;
    }
}

impl Drop for ReplayOwner {
    fn drop(&mut self) {
        if let Some(plan) = self.plan.take() {
            let pusher = Arc::clone(&self.pusher);
            let bus = self.bus.clone();
            n0_future::task::spawn(async move {
                if let Err(error) = pusher
                    .finish_replicator_install(
                        &plan,
                        Some(
                            "initial replay cancelled; add the replicator again to retry"
                                .to_owned(),
                        ),
                        bus,
                    )
                    .await
                {
                    tracing::error!(%error, "failed to persist cancelled replicator install");
                }
            });
        }
    }
}

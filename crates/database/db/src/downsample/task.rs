//! The downsample job: keeps every downsample collection current, from each
//! update event and on a short timer for what events cannot say. It runs in
//! the background on the database's worker pool (`workers::spawn_background`).

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use events::EventName;
use storage::corekv::Store;

/// How often every downsample collection is refreshed whatever the events
/// say.
const REFRESH_EVERY: Duration = Duration::from_millis(250);

/// What woke the job.
enum Wake<M> {
    Tick,
    Event(Option<M>),
}

/// Whichever of `tick` and `event` completes first; `tick` wins a tie.
async fn first<M, T, E>(tick: T, event: E) -> Wake<M>
where
    T: Future<Output = ()>,
    E: Future<Output = Option<M>>,
{
    let (mut tick, mut event) = (pin!(tick), pin!(event));
    std::future::poll_fn(|cx| {
        if tick.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Wake::Tick);
        }
        event.as_mut().poll(cx).map(Wake::Event)
    })
    .await
}

impl<S: Store + 'static> crate::database::DB<S> {
    /// Starts the downsample job on the database's pool. It runs until it is
    /// aborted or its worker stops.
    pub fn start_downsample_task(self: Arc<Self>) -> workers::Background {
        let pool = self.pool().cloned();
        workers::spawn_background(pool.as_ref(), "db::downsample", move || async move {
            self.run_downsamples().await
        })
    }

    /// The job's body, on its worker.
    async fn run_downsamples(self: &Arc<Self>) {
        // The node's own background work runs for no request.
        let context = defra_core::RequestContext::default();
        if let Err(error) = self.bootstrap_downsamples_as(&context, None).await {
            tracing::warn!(error = %error, "Failed to bootstrap downsample collections");
        }
        let mut subscription = self
            .event_bus()
            .map(|bus| bus.subscribe(&[EventName::Update]));
        loop {
            let tick = match workers::sleep(REFRESH_EVERY) {
                Ok(tick) => tick,
                Err(error) => {
                    tracing::error!(%error, "the downsample job stopped: it runs only on a worker");
                    return;
                }
            };
            let wake = match subscription.as_mut() {
                Some(events) => first(tick, events.recv()).await,
                None => {
                    tick.await;
                    Wake::Tick
                }
            };
            let message = match wake {
                Wake::Tick => {
                    if let Err(error) = self.bootstrap_downsamples_as(&context, None).await {
                        tracing::warn!(error = %error, "Failed to refresh downsample collections");
                    }
                    continue;
                }
                // The bus closed: the timer alone keeps the collections
                // current.
                Wake::Event(None) => {
                    subscription = None;
                    continue;
                }
                Wake::Event(Some(message)) => message,
            };
            if subscription
                .as_ref()
                .is_some_and(|events| events.check_and_reset_dropped() > 0)
            {
                tracing::warn!(
                    "Downsample event subscription dropped messages; rebuilding all downsample collections"
                );
                if let Err(error) = self.bootstrap_downsamples_as(&context, None).await {
                    tracing::warn!(
                        error = %error,
                        "Failed to rebuild downsample collections after dropped events"
                    );
                }
            }
            let Some(update) = message.as_update() else {
                continue;
            };
            if let Err(error) = self
                .process_downsample_update(&update.collection_id, &update.doc_id)
                .await
            {
                tracing::warn!(
                    collection_id = %update.collection_id,
                    doc_id = %update.doc_id,
                    error = %error,
                    "Failed to process downsample update"
                );
            }
        }
    }
}

//! Reclaim pages on a schedule (`proofs/tla/ProllyNodeReclaim.tla`).
//!
//! One page of [`ProllyNodes::reclaim`] runs per period, on the worker that
//! runs the schedule. The page's key scan is bounded by `limit`, but marking
//! reads the full closure of every live root, so the transaction still grows
//! with the live trees. Every node commits in the transaction that publishes
//! the root naming it ([`ProllyNodes::publish`]), so a page may run at any
//! moment beside the writers. No production owner starts this schedule yet;
//! it starts with the first real [`LiveRoots`] implementation.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use super::{LiveRoots, ProllyNodes};
use crate::corekv::Store;

/// Runs one page of [`ProllyNodes::reclaim`] against `roots` every `period`
/// on the calling worker, forever. Each page examines at most `limit` node
/// keys and resumes where the last one stopped, while reading every node the
/// live roots reach; the page after the end of `t` starts over from its
/// beginning. A page that fails is logged and runs again, from the same place,
/// a period later. Spawn it as background work
/// ([`workers::spawn_background`]). Fails only off a worker.
pub async fn reclaim_every<S, R>(
    store: Arc<S>,
    roots: Arc<R>,
    period: Duration,
    limit: usize,
) -> Result<Infallible, workers::Error>
where
    S: Store + 'static,
    R: LiveRoots + 'static,
{
    let mut start = None;
    loop {
        workers::sleep(period)?.await;
        match ProllyNodes::reclaim(&*store, Arc::clone(&roots), start.clone(), limit).await {
            Ok(page) => {
                tracing::debug!(
                    scanned = page.scanned,
                    marked = page.marked,
                    deleted = page.deleted,
                    "reclaimed a page of prolly nodes"
                );
                start = page.next_start;
            }
            Err(error) => {
                tracing::warn!(%error, "a prolly node reclaim page failed; it runs again next period");
            }
        }
    }
}

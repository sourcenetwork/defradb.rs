//! Exact rooted CAR authorization, independent of response pagination.
//!
//! Invariant: a CID is granted only once the walk reaches it. Exhaustion
//! fails closed to independent per-block grants; a budget that outlives one
//! request resumes from a retained frontier without re-reading.

use rapidhash::{HashSetExt, RapidHashMap, RapidHashSet};
use std::collections::VecDeque;
use std::time::Duration;

use blockstore::Blockstore;
use cid::Cid;

use crate::error::{Error, Result};

// Bound server work below the transport request deadline. Exhausting this
// budget is a retryable request failure, never a truncated authorization set.
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(2);

/// Nodes attempted per pass; retained cursors resume on subsequent requests.
const AUTHORIZATION_MAX_NODES: usize = 200_000;

/// Incomplete root/want-list walks retained for continuation.
const RETAINED_INCOMPLETE_WALKS: usize = 64;

/// A single traversal pass: what is still looked for, what has been reached,
/// and the not-yet-expanded frontier. Nodes are remembered at push time, so a
/// frontier full of duplicate links costs one queue entry per distinct block.
struct WalkState {
    remaining: RapidHashSet<Cid>,
    authorized: RapidHashSet<Cid>,
    seen: RapidHashSet<Cid>,
    queue: VecDeque<Cid>,
    /// Expansions attempted in this pass, not the retained frontier size.
    expanded: usize,
    /// The node popped for the expansion currently in flight. If the budget
    /// fires mid-read it goes back to the front of the frontier, or the
    /// resumed walk could never pass it: it is already remembered as seen,
    /// so nobody would enqueue it again.
    in_flight: Option<Cid>,
    missing: VecDeque<Cid>,
}

impl WalkState {
    fn fresh(root: Cid, requested: RapidHashSet<Cid>) -> Self {
        let mut walk = Self {
            remaining: requested,
            authorized: RapidHashSet::new(),
            seen: RapidHashSet::new(),
            queue: VecDeque::new(),
            expanded: 0,
            in_flight: None,
            missing: VecDeque::new(),
        };
        walk.push(root);
        walk
    }

    fn push(&mut self, cid: Cid) {
        if self.seen.insert(cid) {
            self.queue.push_back(cid);
        }
    }

    async fn run<B: Blockstore>(
        &mut self,
        blockstore: &B,
        budget: Duration,
        max_nodes: usize,
    ) -> Result<()> {
        // Cancellation drops the future, not the cursor. Retry the interrupted read.
        if let Some(cid) = self.in_flight.take() {
            self.queue.push_front(cid);
        }
        self.expanded = 0;
        let outcome = n0_future::time::timeout(budget, async {
            loop {
                if self.remaining.is_empty() {
                    self.in_flight = None;
                    return Ok(());
                }
                // The budget gate runs before the pop: a CID popped past
                // the budget is already in `seen`, so neither the queue nor
                // the frontier could ever re-offer it to a resumed pass.
                if self.expanded >= max_nodes {
                    return Err(authorization_exhausted(false, self.expanded));
                }
                let Some(cid) = self.queue.pop_front() else {
                    self.in_flight = None;
                    if !self.missing.is_empty() {
                        self.queue.append(&mut self.missing);
                        return Err(Error::ResponseTimeout);
                    }
                    return Ok(());
                };
                self.in_flight = Some(cid);
                self.expanded += 1;
                // In-memory blockstores may never suspend. Yield so the
                // budget and other bounded serve tasks can make progress on
                // deep graphs.
                if self.expanded.is_multiple_of(128) {
                    tokio::task::yield_now().await;
                }
                let size = match blockstore.get_size(&cid).await {
                    Ok(size) => size,
                    // Same rule as budget exhaustion: the CID returns to the
                    // frontier, or the error also deletes it.
                    Err(error) => {
                        return Err(Error::from_blockstore(error));
                    }
                };
                let Some(size) = size else {
                    self.in_flight = None;
                    self.missing.push_back(cid);
                    continue;
                };
                if self.remaining.remove(&cid) {
                    self.authorized.insert(cid);
                }
                // An oversized block is a blob leaf in practice: authorize it
                // if requested, but never read its payload for links, so
                // per-node work stays bounded however large a linked block is.
                if size > crate::sync::car::CAR_MAX_BYTES {
                    self.in_flight = None;
                    continue;
                }
                let data = match blockstore.get(&cid).await {
                    Ok(data) => data,
                    Err(error) => {
                        return Err(Error::from_blockstore(error));
                    }
                };
                let Some(data) = data else {
                    self.in_flight = None;
                    self.missing.push_back(cid);
                    continue;
                };
                // Preserve the existing KMS boundary: encryption links never
                // confer CAR authority, even under an authorized document
                // root.
                let links = super::manager::links::extract_ipld_links(&data).unwrap_or_default();
                for child in links {
                    self.push(child);
                }
                self.in_flight = None;
            }
        })
        .await;
        // Every non-Ok exit — wall-clock timeout, inner error, budget —
        // returns the in-flight CID to the front of the frontier, or the
        // resumed pass could never revisit it.
        if !matches!(outcome, Ok(Ok(()))) {
            if let Some(cid) = self.in_flight.take() {
                self.queue.push_front(cid);
            }
        }
        match outcome {
            Ok(inner) => inner,
            Err(_) => Err(authorization_exhausted(true, self.expanded)),
        }
    }
}

fn authorization_exhausted(timed_out: bool, expanded: usize) -> Error {
    tracing::debug!(
        expanded,
        timed_out,
        "Rooted authorization walk exhausted its budget; failing closed to per-block grants"
    );
    Error::ResponseTimeout
}

/// Independent cursors for a root and an exact requested CID set.
#[derive(Default)]
pub(crate) struct RootedAuthorizationProgress {
    cursors: std::sync::Mutex<CursorRegistry>,
}

#[derive(Default)]
struct CursorRegistry {
    entries: RapidHashMap<WalkKey, std::sync::Arc<tokio::sync::Mutex<WalkState>>>,
    order: VecDeque<WalkKey>,
}

#[derive(Clone, Eq, PartialEq, Hash)]
struct WalkKey {
    root: Cid,
    requested: Vec<Cid>,
}

impl RootedAuthorizationProgress {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn walk<B: Blockstore>(
        &self,
        blockstore: &B,
        root: Cid,
        requested: RapidHashSet<Cid>,
    ) -> Result<RapidHashSet<Cid>> {
        self.walk_with_limits(
            blockstore,
            root,
            requested,
            AUTHORIZATION_TIMEOUT,
            AUTHORIZATION_MAX_NODES,
        )
        .await
    }

    pub(crate) async fn walk_with_limits<B: Blockstore>(
        &self,
        blockstore: &B,
        root: Cid,
        requested: RapidHashSet<Cid>,
        budget: Duration,
        max_nodes: usize,
    ) -> Result<RapidHashSet<Cid>> {
        if requested.is_empty() {
            return Ok(RapidHashSet::new());
        }
        let mut wanted: Vec<_> = requested.iter().copied().collect();
        wanted.sort_unstable();
        let key = WalkKey {
            root,
            requested: wanted,
        };
        let cursor = {
            let mut cursors = self
                .cursors
                .lock()
                .map_err(|_| std::io::Error::other("rooted cursor registry poisoned"))?;
            if let Some(cursor) = cursors.entries.get(&key).cloned() {
                cursors.order.retain(|queued| queued != &key);
                cursors.order.push_back(key.clone());
                cursor
            } else {
                if cursors.entries.len() >= RETAINED_INCOMPLETE_WALKS {
                    let evict = cursors
                        .order
                        .iter()
                        .find(|queued| {
                            cursors
                                .entries
                                .get(*queued)
                                .is_some_and(|cursor| std::sync::Arc::strong_count(cursor) == 1)
                        })
                        .cloned()
                        .ok_or(Error::ResponseTimeout)?;
                    cursors.entries.remove(&evict);
                    cursors.order.retain(|queued| queued != &evict);
                }
                let cursor =
                    std::sync::Arc::new(tokio::sync::Mutex::new(WalkState::fresh(root, requested)));
                cursors.entries.insert(key.clone(), cursor.clone());
                cursors.order.push_back(key.clone());
                cursor
            }
        };
        let started = n0_future::time::Instant::now();
        let mut state = n0_future::time::timeout(budget, cursor.lock())
            .await
            .map_err(|_| Error::ResponseTimeout)?;
        state
            .run(
                blockstore,
                budget.saturating_sub(started.elapsed()),
                max_nodes,
            )
            .await?;
        let authorized = state.authorized.clone();
        // A waiter may still hold this completed cursor after a new one is installed.
        let mut cursors = self
            .cursors
            .lock()
            .map_err(|_| std::io::Error::other("rooted cursor registry poisoned"))?;
        if cursors
            .entries
            .get(&key)
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, &cursor))
        {
            cursors.entries.remove(&key);
            cursors.order.retain(|queued| queued != &key);
        }
        Ok(authorized)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/car_authorization.rs"]
mod tests;

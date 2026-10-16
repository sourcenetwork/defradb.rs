//! Exact rooted CAR authorization, independent of response pagination.
//!
//! Invariant: a CID is granted only once the walk reaches it. Exhaustion
//! fails closed to independent per-block grants; a budget that outlives one
//! request resumes from a retained frontier without re-reading.

use rapidhash::{HashSetExt, RapidHashMap, RapidHashSet};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use blockstore::Blockstore;
use cid::Cid;

use crate::error::{Error, Result};

#[path = "car_authorization_budget.rs"]
mod budget;
use budget::{CidBudget, CidReservation};

// Bound server work below the transport request deadline. Exhausting this
// budget is a retryable request failure, never a truncated authorization set.
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(2);

/// Nodes attempted per pass; retained cursors resume on subsequent requests.
const AUTHORIZATION_MAX_NODES: usize = 200_000;

/// Incomplete root/want-list walks retained for continuation.
const RETAINED_INCOMPLETE_WALKS: usize = 64;

/// Shared by every cursor, including those held by active requests.
const RETAINED_MAX_CIDS: usize = 262_144;
const CURSOR_IDLE_TTL: Duration = Duration::from_secs(300);

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
    reservation: CidReservation,
    retention_exhausted: bool,
}

impl WalkState {
    fn fresh(root: Cid, requested: RapidHashSet<Cid>, reservation: CidReservation) -> Self {
        let mut walk = Self {
            // Do not retain spare capacity supplied by the caller.
            remaining: requested.into_iter().collect(),
            authorized: RapidHashSet::new(),
            seen: RapidHashSet::new(),
            queue: VecDeque::new(),
            expanded: 0,
            in_flight: None,
            missing: VecDeque::new(),
            reservation,
            retention_exhausted: false,
        };
        // The initial reservation includes the root.
        walk.seen.insert(root);
        walk.queue.push_back(root);
        walk
    }

    fn push(&mut self, cid: Cid) -> Result<()> {
        if self.seen.contains(&cid) {
            return Ok(());
        }
        if !self.reservation.add(1) {
            self.retention_exhausted = true;
            return Err(Error::ResponseTimeout);
        }
        self.seen.insert(cid);
        self.queue.push_back(cid);
        Ok(())
    }

    async fn run<B: Blockstore>(
        &mut self,
        blockstore: &B,
        budget: Duration,
        max_nodes: usize,
    ) -> Result<()> {
        if self.retention_exhausted {
            return Err(Error::ResponseTimeout);
        }
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
                if self.remaining.is_empty() {
                    self.in_flight = None;
                    return Ok(());
                }
                // Preserve the existing KMS boundary: encryption links never
                // confer CAR authority, even under an authorized document
                // root.
                let links = super::manager::links::extract_ipld_links(&data).unwrap_or_default();
                for child in links {
                    self.push(child)?;
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
pub(crate) struct RootedAuthorizationProgress {
    cursors: std::sync::Mutex<CursorRegistry>,
    cid_budget: Arc<CidBudget>,
}

impl Default for RootedAuthorizationProgress {
    fn default() -> Self {
        Self {
            cursors: std::sync::Mutex::default(),
            cid_budget: CidBudget::new(RETAINED_MAX_CIDS),
        }
    }
}

#[derive(Default)]
struct CursorRegistry {
    entries: RapidHashMap<WalkKey, CursorEntry>,
    order: VecDeque<WalkKey>,
}

struct CursorEntry {
    cursor: Arc<tokio::sync::Mutex<WalkState>>,
    last_used: n0_future::time::Instant,
}

impl CursorRegistry {
    fn expire_idle(&mut self) {
        self.entries.retain(|_, entry| {
            entry.last_used.elapsed() < CURSOR_IDLE_TTL || Arc::strong_count(&entry.cursor) > 1
        });
        self.order.retain(|key| self.entries.contains_key(key));
    }

    fn evict_idle(&mut self) -> bool {
        let Some(key) = self
            .order
            .iter()
            .find(|key| {
                self.entries
                    .get(*key)
                    .is_some_and(|entry| Arc::strong_count(&entry.cursor) == 1)
            })
            .cloned()
        else {
            return false;
        };
        self.remove(&key);
        true
    }

    fn remove(&mut self, key: &WalkKey) {
        self.entries.remove(key);
        self.order.retain(|queued| queued != key);
    }
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
        if requested.len() >= self.cid_budget.limit {
            return Err(Error::ResponseTimeout);
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
            cursors.expire_idle();
            if let Some(entry) = cursors.entries.get_mut(&key) {
                entry.last_used = n0_future::time::Instant::now();
                let cursor = entry.cursor.clone();
                cursors.order.retain(|queued| queued != &key);
                cursors.order.push_back(key.clone());
                cursor
            } else {
                if cursors.entries.len() >= RETAINED_INCOMPLETE_WALKS && !cursors.evict_idle() {
                    return Err(Error::ResponseTimeout);
                }
                let reservation = loop {
                    if let Some(reservation) = self.cid_budget.reserve(requested.len() + 1) {
                        break reservation;
                    }
                    if !cursors.evict_idle() {
                        return Err(Error::ResponseTimeout);
                    }
                };
                let cursor = Arc::new(tokio::sync::Mutex::new(WalkState::fresh(
                    root,
                    requested,
                    reservation,
                )));
                cursors.entries.insert(
                    key.clone(),
                    CursorEntry {
                        cursor: cursor.clone(),
                        last_used: n0_future::time::Instant::now(),
                    },
                );
                cursors.order.push_back(key.clone());
                cursor
            }
        };
        let started = n0_future::time::Instant::now();
        let mut state = n0_future::time::timeout(budget, cursor.lock())
            .await
            .map_err(|_| Error::ResponseTimeout)?;
        let outcome = state
            .run(
                blockstore,
                budget.saturating_sub(started.elapsed()),
                max_nodes,
            )
            .await;
        let retain = !state.retention_exhausted
            && (outcome.is_err()
                || (!state.remaining.is_empty()
                    && (!state.queue.is_empty()
                        || !state.missing.is_empty()
                        || state.in_flight.is_some())));
        // A waiter may still hold this completed cursor after a new one is installed.
        let mut cursors = self
            .cursors
            .lock()
            .map_err(|_| std::io::Error::other("rooted cursor registry poisoned"))?;
        if let Some(entry) = cursors
            .entries
            .get_mut(&key)
            .filter(|entry| Arc::ptr_eq(&entry.cursor, &cursor))
        {
            if retain {
                entry.last_used = n0_future::time::Instant::now();
            } else {
                cursors.remove(&key);
            }
        }
        outcome.map(|()| state.authorized.clone())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/car_authorization.rs"]
mod tests;

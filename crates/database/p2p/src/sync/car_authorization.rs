//! Exact rooted CAR authorization, independent of response pagination.
//!
//! Invariant: a CID is granted only once the walk reaches it. Exhaustion
//! fails closed to independent per-block grants; a budget that outlives one
//! request resumes from a retained frontier without re-reading.

use rapidhash::{HashSetExt, RapidHashSet};
use std::collections::VecDeque;
use std::time::Duration;

use blockstore::Blockstore;
use cid::Cid;

use crate::error::{Error, Result};

#[path = "car_authorization_budget.rs"]
mod budget;
#[path = "car_authorization_registry.rs"]
mod registry;
pub(crate) use registry::RootedAuthorizationProgress;

// Bound server work below the transport request deadline. Exhausting this
// budget is a retryable request failure, never a truncated authorization set.
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(2);

/// Nodes attempted per pass; retained cursors resume on subsequent requests.
const AUTHORIZATION_MAX_NODES: usize = 200_000;

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
            // Do not retain spare capacity supplied by the caller.
            remaining: requested.into_iter().collect(),
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

    fn can_resume(&self) -> bool {
        !self.queue.is_empty() || !self.missing.is_empty() || self.in_flight.is_some()
    }

    async fn run<B>(&mut self, blockstore: &B, budget: Duration, max_nodes: usize) -> Result<()>
    where
        B: Blockstore,
    {
        // Cancellation drops the future, not the cursor. Retry the interrupted read.
        if let Some(cid) = self.in_flight.take() {
            self.queue.push_front(cid);
        }
        self.expanded = 0;
        let outcome = crate::rt::time::timeout(budget, async {
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
                    crate::rt::yield_now().await;
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
                    self.push(child);
                }
                self.in_flight = None;
            }
        })
        .await;
        // Every non-Ok exit - wall-clock timeout, inner error, budget -
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

#[cfg(test)]
#[path = "../../tests/unit/car_authorization.rs"]
mod tests;

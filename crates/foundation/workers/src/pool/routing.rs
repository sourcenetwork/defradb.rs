//! Which worker a submission goes to. No request hops from worker to worker:
//! the target is chosen once, here, at the edge.
//!
//! - [`Target::Worker`]: the owner (an explicit transaction's worker). Bound:
//!   it waits for its owner, even a stuck one.
//! - [`Target::Key`]: document affinity. A hash picks the worker, which runs
//!   one task per key at a time. While that worker is stuck, the key's jobs
//!   go to one fallback on the same node, the same one for every job of the
//!   key.
//! - [`Target::LeastLoaded`]: the less loaded of two random workers, from the
//!   caller's own node when it has workers there.
//! - [`Target::Node`]: the same, among one node's workers.
//!
//! Stuck workers are skipped for everything but the owner.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::PoolShared;
use crate::worker::{Admitted, Job, WorkerShared};
use crate::Error;

/// Where a submission runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// This worker, by index: the owner of the work.
    Worker(usize),
    /// The worker a hash of this key picks, one task per key at a time.
    Key(u64),
    /// Whichever worker is less loaded, preferring the caller's node.
    LeastLoaded,
    /// The least loaded worker on this NUMA node.
    Node(usize),
}

/// Maps `hash` onto `0..n` evenly (Lemire's multiply-shift).
fn spread(hash: u64, n: usize) -> usize {
    ((u128::from(hash) * n as u128) >> 64) as usize
}

/// A per-thread xorshift: two random picks per submission need no shared
/// state.
fn random() -> u64 {
    static SEED: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(0) };
    }
    STATE.with(|state| {
        let mut x = state.get();
        if x == 0 {
            x = SEED.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed) | 1;
        }
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state.set(x);
        x
    })
}

fn stuck(worker: &WorkerShared) -> bool {
    worker.stuck.load(Ordering::Relaxed)
}

impl PoolShared {
    /// The worker `target` names now.
    pub(crate) fn route(&self, target: Target) -> Result<&Arc<WorkerShared>, Error> {
        match target {
            Target::Worker(index) => self.workers.get(index).ok_or(Error::NoSuchWorker {
                index,
                workers: self.workers.len(),
            }),
            Target::Key(hash) => Ok(self.key_worker(hash)),
            Target::LeastLoaded => Ok(self.least_loaded(self.caller_candidates())),
            Target::Node(node) => self
                .node_workers(node)
                .map(|candidates| self.least_loaded(candidates))
                .ok_or(Error::NoSuchNode(node)),
        }
    }

    /// Room on some worker `target` allows, taken now without waiting:
    /// unbound targets try every candidate, nearest first.
    pub(crate) fn admit_anywhere(&self, target: Target, bytes: usize) -> Option<Admitted> {
        let candidates = match target {
            Target::Worker(_) | Target::Key(_) => return None,
            Target::LeastLoaded => self.caller_candidates(),
            Target::Node(node) => self.node_workers(node)?,
        };
        candidates
            .iter()
            .map(|index| &self.workers[*index])
            .filter(|worker| !stuck(worker))
            .find_map(|worker| worker.try_admit(bytes))
    }

    /// The key's home worker, or while it is stuck, the key's fallback among
    /// the home node's other workers.
    pub(crate) fn key_worker(&self, hash: u64) -> &Arc<WorkerShared> {
        let home = &self.workers[spread(hash, self.workers.len())];
        if !stuck(home) {
            return home;
        }
        let peers = || {
            self.node_workers(home.id.node)
                .into_iter()
                .flatten()
                .map(|index| &self.workers[*index])
                .filter(|worker| !stuck(worker))
        };
        let count = peers().count();
        if count == 0 {
            return home;
        }
        peers()
            .nth(spread(hash.rotate_left(32), count))
            .unwrap_or(home)
    }

    /// The less loaded of two random candidates, skipping stuck workers.
    fn least_loaded(&self, candidates: &[usize]) -> &Arc<WorkerShared> {
        let pick = || &self.workers[candidates[spread(random(), candidates.len())]];
        let (a, b) = (pick(), pick());
        match (stuck(a), stuck(b)) {
            (false, false) if b.load() < a.load() => b,
            (false, _) => a,
            (true, false) => b,
            (true, true) => candidates
                .iter()
                .map(|index| &self.workers[*index])
                .find(|worker| !stuck(worker))
                .unwrap_or(a),
        }
    }

    /// The caller's node's workers when it has any, else every worker.
    fn caller_candidates(&self) -> &[usize] {
        crate::os::system()
            .current_cpu()
            .and_then(|cpu| self.cpu_node.get(cpu).copied().flatten())
            .and_then(|node| self.node_workers(node))
            .unwrap_or(&self.all)
    }

    fn node_workers(&self, node: usize) -> Option<&[usize]> {
        self.nodes
            .iter()
            .find(|(id, _)| *id == node)
            .map(|(_, workers)| workers.as_slice())
    }

    /// Moves a queued job off the stuck worker `from` to a free worker on the
    /// same node with room; hands it back when there is none.
    pub(crate) fn reroute(&self, mut job: Job, from: &WorkerShared) -> Result<(), Job> {
        let target = match job.key {
            Some(key) => self.key_worker(key),
            None => match self
                .node_workers(from.id.node)
                .into_iter()
                .flatten()
                .map(|index| &self.workers[*index])
                .filter(|worker| !stuck(worker))
                .min_by_key(|worker| worker.load())
            {
                Some(worker) => worker,
                None => return Err(job),
            },
        };
        if target.id == from.id {
            return Err(job);
        }
        let Some(admitted) = target.try_admit(job.admitted.bytes()) else {
            return Err(job);
        };
        job.admitted = admitted;
        target.push(job);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_covers_every_worker_evenly() {
        let mut hits = [0usize; 4];
        for i in 0..4000u64 {
            hits[spread(i.wrapping_mul(0x9e37_79b9_7f4a_7c15), 4)] += 1;
        }
        assert!(hits.iter().all(|h| (900..1100).contains(h)), "{hits:?}");
        assert_eq!(spread(u64::MAX, 3), 2);
        assert_eq!(spread(0, 3), 0);
    }

    #[test]
    fn random_differs_between_calls() {
        assert_ne!(random(), random());
    }
}

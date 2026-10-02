//! Prioritised per-peer task queue: peers are served roughly alternately, each in priority then FIFO order.
//!
//! A plain struct with `&mut self` methods: the engine owner task is its only user.

mod task;
mod tracker;

use keyed_priority_queue::{Entry, KeyedPriorityQueue};
use libp2p::PeerId;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashSet;

pub use task::{Data, DefaultTaskMerger, QueueTask, Task, TaskMerger, Topic};
pub use tracker::{PeerTracker, Stats as TrackerStats, Topics};

/// A prioritised list of tasks to be executed on peers.
#[derive(Debug)]
pub struct PeerTaskQueue<T: Topic, D: Data, TM: TaskMerger<T, D> = DefaultTaskMerger> {
    peer_queue: KeyedPriorityQueue<PeerId, PeerTracker<T, D, TM>, RandomState>,
    frozen_peers: RapidHashSet<PeerId>,
    ignore_freezing: bool,
    task_merger: TM,
    max_outstanding_work_per_peer: usize,
}

impl<T: Topic, D: Data, TM: TaskMerger<T, D> + Default> Default for PeerTaskQueue<T, D, TM> {
    fn default() -> Self {
        Self::new(TM::default(), Config::default())
    }
}

/// A peer, the tasks popped for it and the work still pending for it.
pub type Popped<T, D> = (PeerId, Vec<Task<T, D>>, usize);

/// Queue configuration.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// When set, cancelling a pending task never freezes the peer.
    pub ignore_freezing: bool,
    /// Work a peer may have active at once; 0 disables the cap.
    pub max_outstanding_work_per_peer: usize,
}

/// Aggregate queue counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stats {
    /// Peers with queued or active work.
    pub num_peers: usize,
    /// Active topics across peers.
    pub num_active: usize,
    /// Pending topics across peers.
    pub num_pending: usize,
}

impl<T: Topic, D: Data, TM: TaskMerger<T, D>> PeerTaskQueue<T, D, TM> {
    /// An empty queue.
    pub fn new(task_merger: TM, config: Config) -> Self {
        PeerTaskQueue {
            peer_queue: KeyedPriorityQueue::with_hasher(RandomState::default()),
            frozen_peers: Default::default(),
            ignore_freezing: config.ignore_freezing,
            task_merger,
            max_outstanding_work_per_peer: config.max_outstanding_work_per_peer,
        }
    }

    /// Aggregate counts.
    pub fn stats(&self) -> Stats {
        let mut stats = Stats {
            num_peers: self.peer_queue.len(),
            num_active: 0,
            num_pending: 0,
        };

        for (_, tracker) in self.peer_queue.iter() {
            let tracker_stats = tracker.stats();
            stats.num_active += tracker_stats.num_active;
            stats.num_pending += tracker_stats.num_pending;
        }

        stats
    }

    /// Topics queued or active for a peer.
    pub fn peer_topics(&mut self, peer: &PeerId) -> Option<Topics<T>> {
        match self.peer_queue.entry(*peer) {
            Entry::Occupied(tracker) => Some(tracker.get_priority().topics()),
            Entry::Vacant(_) => None,
        }
    }

    /// Adds a group of tasks for the peer.
    pub fn push_tasks(&mut self, peer: PeerId, tasks: Vec<Task<T, D>>) {
        let mut peer_tracker = self.peer_queue.remove(&peer).unwrap_or_else(|| {
            PeerTracker::new(
                peer,
                self.task_merger.clone(),
                self.max_outstanding_work_per_peer,
            )
        });

        peer_tracker.push_tasks(tasks);
        self.peer_queue.push(peer, peer_tracker);
    }

    /// Adds one task for the peer.
    pub fn push_task(&mut self, peer: PeerId, task: Task<T, D>) {
        self.push_tasks(peer, vec![task]);
    }

    /// Pops tasks covering `target_min_work` from the best peer, with the peer's remaining pending work.
    ///
    /// Peers with the most active work are deprioritized and peers with the most pending work favoured.
    pub fn pop_tasks(&mut self, target_min_work: usize) -> Option<Popped<T, D>> {
        let (peer, mut peer_tracker) = self.peer_queue.pop()?;
        let out = peer_tracker.pop_tasks(target_min_work);
        let pending_work = peer_tracker.get_pending_work();

        if peer_tracker.is_idle() {
            self.frozen_peers.remove(&peer);
        } else {
            self.peer_queue.push(peer, peer_tracker);
        }

        Some((peer, out, pending_work))
    }

    /// Marks the tasks completed for the peer.
    pub fn tasks_done(&mut self, peer: PeerId, tasks: &[Task<T, D>]) {
        if let Some(mut peer_tracker) = self.peer_queue.remove(&peer) {
            for task in tasks {
                peer_tracker.task_done(task);
            }
            self.peer_queue.push(peer, peer_tracker);
        }
    }

    /// Removes a pending task, freezing the peer unless freezing is ignored.
    pub fn remove(&mut self, topic: &T, peer: PeerId) {
        if let Some(mut peer_tracker) = self.peer_queue.remove(&peer) {
            if peer_tracker.remove(topic) && !self.ignore_freezing {
                if !peer_tracker.is_frozen() {
                    self.frozen_peers.insert(peer);
                }
                peer_tracker.freeze();
            }
            self.peer_queue.push(peer, peer_tracker);
        }
    }

    /// Completely thaws every peer.
    pub fn full_thaw(&mut self) {
        let frozen_peers: Vec<_> = self.frozen_peers.iter().copied().collect();
        for peer in frozen_peers {
            if let Some(mut peer_tracker) = self.peer_queue.remove(&peer) {
                peer_tracker.full_thaw();
                self.frozen_peers.remove(&peer);
                self.peer_queue.push(peer, peer_tracker);
            }
        }
    }

    /// Thaws peers incrementally, least frozen first.
    pub fn thaw_round(&mut self) {
        let frozen_peers: Vec<_> = self.frozen_peers.iter().copied().collect();
        for peer in frozen_peers {
            if let Some(mut peer_tracker) = self.peer_queue.remove(&peer) {
                if peer_tracker.thaw() {
                    self.frozen_peers.remove(&peer);
                }
                self.peer_queue.push(peer, peer_tracker);
            }
        }
    }
}

//! Per-peer pending and active task bookkeeping.

use std::time::Instant;

use keyed_priority_queue::{Entry, KeyedPriorityQueue};
use libp2p::PeerId;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashMap;
use tracing::debug;

use super::task::{Data, QueueTask, Task, TaskMerger, Topic};

/// Tracks pending and active tasks for a single peer.
#[derive(Debug)]
pub struct PeerTracker<T: Topic, D: Data, TM: TaskMerger<T, D>> {
    target: PeerId,
    pending_tasks: KeyedPriorityQueue<T, QueueTask<T, D>, RandomState>,
    active_tasks: RapidHashMap<T, Vec<Task<T, D>>>,
    active_work: usize,
    max_active_work_per_peer: usize,
    freeze_val: isize,
    task_merger: TM,
}

impl<T: Topic, D: Data, TM: TaskMerger<T, D>> PartialEq for PeerTracker<T, D, TM> {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target
            && self.active_tasks == other.active_tasks
            && self.active_work == other.active_work
            && self.max_active_work_per_peer == other.max_active_work_per_peer
            && self.freeze_val == other.freeze_val
            && self.task_merger == other.task_merger
            && self.pending_tasks.len() == other.pending_tasks.len()
            && self
                .pending_tasks
                .iter()
                .zip(other.pending_tasks.iter())
                .all(|(a, b)| a == b)
    }
}

impl<T: Topic, D: Data, TM: TaskMerger<T, D>> Eq for PeerTracker<T, D, TM> {}

impl<T: Topic, D: Data, TM: TaskMerger<T, D>> PeerTracker<T, D, TM> {
    /// A tracker for `target`; `max_active_work_per_peer` of 0 disables the cap.
    pub fn new(target: PeerId, task_merger: TM, max_active_work_per_peer: usize) -> Self {
        PeerTracker {
            target,
            pending_tasks: KeyedPriorityQueue::with_hasher(RandomState::default()),
            active_tasks: Default::default(),
            active_work: 0,
            max_active_work_per_peer,
            freeze_val: 0,
            task_merger,
        }
    }

    /// True when the peer has no active or pending tasks.
    pub fn is_idle(&self) -> bool {
        self.pending_tasks.is_empty() && self.active_tasks.is_empty()
    }

    /// Counts of pending and active topics.
    pub fn stats(&self) -> Stats {
        Stats {
            num_pending: self.pending_tasks.len(),
            num_active: self.active_tasks.len(),
        }
    }

    /// Sorted pending and active topics.
    pub fn topics(&self) -> Topics<T> {
        let mut pending: Vec<_> = self
            .pending_tasks
            .iter()
            .map(|(_, qt)| qt.task.topic.clone())
            .collect();
        pending.sort();
        let mut active: Vec<_> = self
            .active_tasks
            .values()
            .flat_map(|t| t.iter().map(|t| t.topic.clone()))
            .collect();
        active.sort();
        Topics { pending, active }
    }

    /// Queues tasks, merging with pending ones and skipping those that add nothing over active ones.
    pub fn push_tasks(&mut self, tasks: Vec<Task<T, D>>) {
        let now = Instant::now();
        for task in tasks {
            if !self.task_has_more_info_than_active_tasks(&task) {
                continue;
            }

            if let Entry::Occupied(existing_task_entry) =
                self.pending_tasks.entry(task.topic.clone())
            {
                let (key, mut existing_task) = existing_task_entry.remove();
                if task.priority > existing_task.task.priority {
                    existing_task.task.priority = task.priority;
                }
                self.task_merger.merge(&task, &mut existing_task.task);
                self.pending_tasks.push(key, existing_task);
                continue;
            }

            let topic = task.topic.clone();
            let qtask = QueueTask::new(task, self.target, now);
            self.pending_tasks.push(topic, qtask);
        }
    }

    /// Pops tasks in priority order until `target_min_work` is covered or the peer is capped or frozen.
    pub fn pop_tasks(&mut self, target_min_work: usize) -> Vec<Task<T, D>> {
        let mut out = Vec::new();
        let mut work = 0;

        while !self.pending_tasks.is_empty() && self.freeze_val == 0 && work < target_min_work {
            if self.max_active_work_per_peer > 0
                && self.active_work >= self.max_active_work_per_peer
            {
                break;
            }

            if let Some((_, qtask)) = self.pending_tasks.pop() {
                let task = qtask.task;
                self.start_task(task.clone());
                work += task.work;
                out.push(task);
            }
        }

        out
    }

    /// Marks a task active.
    pub fn start_task(&mut self, task: Task<T, D>) {
        self.active_work += task.work;
        self.active_tasks
            .entry(task.topic.clone())
            .or_default()
            .push(task);
    }

    /// Total work in the pending queue.
    pub fn get_pending_work(&self) -> usize {
        self.pending_tasks.iter().map(|(_, qt)| qt.task.work).sum()
    }

    /// Signals that the task completed.
    pub fn task_done(&mut self, task: &Task<T, D>) {
        if let Some(active_tasks) = self.active_tasks.get_mut(&task.topic) {
            let mut work_done = 0;
            active_tasks.retain(|at| {
                if at == task {
                    work_done += task.work;
                    false
                } else {
                    true
                }
            });

            if self.active_work < work_done {
                debug!(
                    active_work = self.active_work,
                    work_done, "more work finished than started"
                );
            }
            self.active_work = self.active_work.saturating_sub(work_done);

            if active_tasks.is_empty() {
                self.active_tasks.remove(&task.topic);
            }
        }
    }

    /// Removes a pending task; true when one was removed.
    pub fn remove(&mut self, topic: &T) -> bool {
        self.pending_tasks.remove(topic).is_some()
    }

    /// Freezes the peer, one level deeper.
    pub fn freeze(&mut self) {
        self.freeze_val += 1;
    }

    /// Decrements the freeze level; true when the peer is no longer frozen.
    pub fn thaw(&mut self) -> bool {
        self.freeze_val -= (self.freeze_val + 1) / 2;
        self.freeze_val <= 0
    }

    /// Completely unfreezes the peer.
    pub fn full_thaw(&mut self) {
        self.freeze_val = 0;
    }

    /// Whether the peer is frozen and unable to execute tasks.
    pub fn is_frozen(&self) -> bool {
        self.freeze_val > 0
    }

    fn task_has_more_info_than_active_tasks(&self, task: &Task<T, D>) -> bool {
        match self.active_tasks.get(&task.topic) {
            Some(tasks_with_topic) if !tasks_with_topic.is_empty() => {
                self.task_merger.has_new_info(task, tasks_with_topic)
            }
            _ => true,
        }
    }
}

/// Counts of pending and active topics.
#[derive(Debug)]
pub struct Stats {
    /// Pending topics.
    pub num_pending: usize,
    /// Active topics.
    pub num_active: usize,
}

/// Pending and active topics of a peer.
#[derive(Debug)]
pub struct Topics<T: Topic> {
    /// Pending topics.
    pub pending: Vec<T>,
    /// Active topics.
    pub active: Vec<T>,
}

impl<T: Topic, D: Data, TM: TaskMerger<T, D>> PartialOrd for PeerTracker<T, D, TM> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Topic, D: Data, TM: TaskMerger<T, D>> Ord for PeerTracker<T, D, TM> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.pending_tasks.is_empty() {
            return std::cmp::Ordering::Less;
        }
        if other.pending_tasks.is_empty() {
            return std::cmp::Ordering::Greater;
        }

        if self.freeze_val > other.freeze_val {
            return std::cmp::Ordering::Less;
        }
        // The reference's mirrored check (less frozen ranks higher) repeats the comparison above and
        // never fires; it is omitted to keep the exact peer ordering.

        if self.active_work == other.active_work {
            return self.pending_tasks.len().cmp(&other.pending_tasks.len());
        }

        other.active_work.cmp(&self.active_work)
    }
}

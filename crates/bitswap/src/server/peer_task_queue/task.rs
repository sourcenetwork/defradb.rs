//! Tasks, their queue bookkeeping and the merge policy.

use std::fmt::Debug;
use std::hash::Hash;
use std::time::Instant;

use libp2p::PeerId;

/// A non-unique name for a task.
pub trait Topic: Sized + Debug + Clone + Eq + Ord + Hash {}
impl<T: Sized + Debug + Clone + Eq + Ord + Hash> Topic for T {}

/// Metadata attached to a task.
pub trait Data: Sized + Debug + Clone + Eq + Send {}
impl<D: Sized + Debug + Clone + Eq + Send> Data for D {}

/// A single task to be executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task<T: Topic, D: Data> {
    /// The topic of the task.
    pub topic: T,
    /// The priority of the task.
    pub priority: isize,
    /// Peers with the most active work are deprioritized, peers with the most pending work favoured.
    pub work: usize,
    /// Associated data.
    pub data: D,
}

/// A task plus the bookkeeping the tracker needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueTask<T: Topic, D: Data> {
    /// The task.
    pub task: Task<T, D>,
    /// The peer the task is for.
    pub target: PeerId,
    /// When the task entered the queue.
    pub created: Instant,
}

impl<T: Topic, D: Data> PartialOrd for QueueTask<T, D> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Topic, D: Data> Ord for QueueTask<T, D> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.target == other.target && self.task.priority != other.task.priority {
            return self.task.priority.cmp(&other.task.priority);
        }

        other.created.cmp(&self.created)
    }
}

impl<T: Topic, D: Data> QueueTask<T, D> {
    /// Wraps a task.
    pub fn new(task: Task<T, D>, target: PeerId, created: Instant) -> Self {
        QueueTask {
            task,
            target,
            created,
        }
    }
}

/// Decides how a new task merges into the active and pending queues.
pub trait TaskMerger<T: Topic, D: Data>:
    PartialEq + Eq + Clone + std::fmt::Debug + Send + Sync + 'static
{
    /// Whether the task carries more information than the existing tasks with the same topic.
    fn has_new_info(&self, task_info: &Task<T, D>, existing_tasks: &[Task<T, D>]) -> bool;
    /// Copies relevant fields from a new task into an existing one.
    fn merge(&self, task: &Task<T, D>, existing: &mut Task<T, D>);
}

/// A merger that never merges.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct DefaultTaskMerger {}

impl<T: Topic, D: Data> TaskMerger<T, D> for DefaultTaskMerger {
    fn has_new_info(&self, _task_info: &Task<T, D>, _existing_tasks: &[Task<T, D>]) -> bool {
        false
    }
    fn merge(&self, _task: &Task<T, D>, _existing: &mut Task<T, D>) {}
}

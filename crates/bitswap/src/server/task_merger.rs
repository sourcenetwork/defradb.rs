//! Task data and the merge policy the engine uses to collapse repeated wants for one block.

use cid::Cid;

use super::peer_task_queue::{self, Task};

/// Extra data associated with each task in the request queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskData {
    /// Tasks are either want-have or want-block.
    pub is_want_block: bool,
    /// Whether to send a DONT_HAVE when the block is missing.
    pub send_dont_have: bool,
    /// The size of the block.
    pub block_size: usize,
    /// Whether the block was found.
    pub have_block: bool,
}

/// Merge policy: want-block beats want-have, known size beats unknown.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct TaskMerger {}

impl peer_task_queue::TaskMerger<Cid, TaskData> for TaskMerger {
    fn has_new_info(&self, task: &Task<Cid, TaskData>, existing: &[Task<Cid, TaskData>]) -> bool {
        let have_size = existing.iter().any(|entry| entry.data.have_block);
        let is_want_block = existing.iter().any(|entry| entry.data.is_want_block);

        (!is_want_block && task.data.is_want_block) || (!have_size && task.data.have_block)
    }

    fn merge(&self, task: &Task<Cid, TaskData>, existing: &mut Task<Cid, TaskData>) {
        let new_task = &task.data;
        let existing_task = &mut existing.data;

        if !existing_task.have_block && new_task.have_block {
            existing_task.have_block = new_task.have_block;
            existing_task.block_size = new_task.block_size;
        }

        if !existing_task.is_want_block && new_task.is_want_block {
            existing_task.is_want_block = true;
            if !existing_task.have_block || new_task.have_block {
                existing_task.have_block = new_task.have_block;
                existing.work = task.work;
            }
        }

        // The whole block is sent, so the work is the block size.
        if existing_task.is_want_block && existing_task.have_block {
            existing.work = existing_task.block_size;
        }
    }
}

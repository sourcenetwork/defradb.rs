//! Document affinity: a worker runs one keyed task per key at a time, the rest
//! for that key waiting in arrival order. One thread owns it, so it needs no
//! lock; two tasks for one document never interleave at an `.await` and so
//! never conflict with each other.

use std::collections::hash_map::Entry;
use std::collections::VecDeque;

use rapidhash::fast::RapidHashMap;

/// Keys with a task running, and the jobs waiting behind each.
pub(crate) struct Keyed<J> {
    waiting: RapidHashMap<u64, VecDeque<J>>,
}

impl<J> Keyed<J> {
    pub(crate) fn new() -> Keyed<J> {
        Keyed {
            waiting: RapidHashMap::default(),
        }
    }

    /// `job` if it may start now; otherwise it waits behind the key's
    /// running task.
    pub(crate) fn admit(&mut self, key: u64, job: J) -> Option<J> {
        match self.waiting.entry(key) {
            Entry::Occupied(mut running) => {
                running.get_mut().push_back(job);
                None
            }
            Entry::Vacant(free) => {
                free.insert(VecDeque::new());
                Some(job)
            }
        }
    }

    /// The task for `key` finished: the next job for it, which now runs.
    pub(crate) fn finished(&mut self, key: u64) -> Option<J> {
        let queue = self.waiting.get_mut(&key)?;
        let next = queue.pop_front();
        if next.is_none() {
            self.waiting.remove(&key);
        }
        next
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    /// Every waiting job, for shutdown.
    pub(crate) fn drain(&mut self) -> Vec<J> {
        self.waiting.drain().flat_map(|(_, queue)| queue).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_task_per_key_runs_and_the_rest_follow_in_order() {
        let mut keyed = Keyed::new();
        assert_eq!(keyed.admit(1, "a1"), Some("a1"));
        assert_eq!(keyed.admit(2, "b1"), Some("b1"), "another key runs at once");
        assert_eq!(keyed.admit(1, "a2"), None);
        assert_eq!(keyed.admit(1, "a3"), None);
        assert_eq!(keyed.finished(1), Some("a2"));
        assert_eq!(keyed.finished(1), Some("a3"));
        assert_eq!(keyed.finished(1), None);
        assert_eq!(keyed.admit(1, "a4"), Some("a4"), "the key is free again");
        assert_eq!(keyed.finished(2), None);
        assert_eq!(keyed.admit(3, "c1"), Some("c1"));
        assert_eq!(keyed.admit(3, "c2"), None);
        assert_eq!(keyed.drain(), vec!["c2"]);
        assert!(keyed.is_empty());
    }
}

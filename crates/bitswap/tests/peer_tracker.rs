#![allow(clippy::useless_vec)]

use std::time::Instant;

use bitswap::server::peer_task_queue::{
    Data, DefaultTaskMerger, PeerTracker, Task, TaskMerger, Topic,
};
use libp2p::PeerId;

fn wait_for_new_instant() {
    let start = Instant::now();
    while Instant::now() <= start {
        std::hint::spin_loop();
    }
}

const MAX_ACTIVE_WORK_PER_PEER: usize = 100;

#[test]
fn test_empty() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<&'static [u8], (), _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = tracker.pop_tasks(100);
    assert!(tasks.is_empty());
}

#[test]
fn test_push_pop() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<&'static [u8], (), _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![Task {
        topic: &b"1"[..],
        priority: 1,
        work: 10,
        data: (),
    }];
    tracker.push_tasks(tasks);

    let popped = tracker.pop_tasks(100);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].topic, b"1");
}

#[test]
fn test_pop_zero_size() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<&'static [u8], (), _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![Task {
        topic: &b"1"[..],
        priority: 1,
        work: 10,
        data: (),
    }];
    tracker.push_tasks(tasks);

    let popped = tracker.pop_tasks(0);
    assert!(popped.is_empty());
}

#[test]
fn test_pop_size_order() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, (), _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: (),
        },
        Task {
            topic: 2,
            priority: 20,
            work: 10,
            data: (),
        },
        Task {
            topic: 3,
            priority: 15,
            work: 10,
            data: (),
        },
    ];
    tracker.push_tasks(tasks);

    let popped = tracker.pop_tasks(10);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].topic, 2);
    assert_eq!(tracker.get_pending_work(), 20);

    let topics = tracker.topics();
    assert_eq!(topics.active.len(), 1);
    assert_eq!(topics.active[0], popped[0].topic);

    assert_eq!(topics.pending.len(), 2);
    assert_eq!(topics.pending[0], 1);
    assert_eq!(topics.pending[1], 3);

    let popped = tracker.pop_tasks(100);
    assert_eq!(popped.len(), 2);
    assert_eq!(popped[0].topic, 3);
    assert_eq!(popped[1].topic, 1);
    assert_eq!(tracker.get_pending_work(), 0);

    let topics = tracker.topics();
    assert_eq!(topics.active, [1, 2, 3]);
    assert!(topics.pending.is_empty());

    let popped = tracker.pop_tasks(100);
    assert!(popped.is_empty());
    assert_eq!(tracker.get_pending_work(), 0);
}

#[test]
fn test_pop_first_item_always() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, (), _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 20,
            work: 10,
            data: (),
        },
        Task {
            topic: 2,
            priority: 10,
            work: 5,
            data: (),
        },
    ];
    tracker.push_tasks(tasks);

    // should always return the first task, even if it's under target work
    let popped = tracker.pop_tasks(7);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].topic, 1);

    let popped = tracker.pop_tasks(100);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].topic, 2);
}

#[test]
fn test_pop_items_to_cover_target_work() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, (), _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 20,
            work: 5,
            data: (),
        },
        Task {
            topic: 2,
            priority: 10,
            work: 5,
            data: (),
        },
        Task {
            topic: 3,
            priority: 5,
            work: 5,
            data: (),
        },
    ];
    tracker.push_tasks(tasks);

    // should always return the first task, even if it's under target work
    let popped = tracker.pop_tasks(7);
    assert_eq!(popped.len(), 2);
    assert_eq!(popped[0].topic, 1);
    assert_eq!(popped[1].topic, 2);

    let popped = tracker.pop_tasks(100);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].topic, 3);
}

#[test]
fn test_single_remove() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, (), _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: (),
        },
        Task {
            topic: 2,
            priority: 20,
            work: 10,
            data: (),
        },
        Task {
            topic: 3,
            priority: 15,
            work: 10,
            data: (),
        },
    ];
    tracker.push_tasks(tasks);

    tracker.remove(&2);

    let popped = tracker.pop_tasks(100);
    assert_eq!(popped.len(), 2);
    assert_eq!(popped[0].topic, 3);
    assert_eq!(popped[1].topic, 1);
}

#[test]
fn test_multi_remove() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, (), _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: (),
        },
        Task {
            topic: 1,
            priority: 20,
            work: 1,
            data: (),
        },
        Task {
            topic: 2,
            priority: 15,
            work: 10,
            data: (),
        },
    ];
    tracker.push_tasks(tasks);

    tracker.remove(&1);

    let popped = tracker.pop_tasks(100);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].topic, 2);
}

#[test]
fn test_task_done() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, _, _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: "a",
        },
        Task {
            topic: 2,
            priority: 20,
            work: 10,
            data: "b",
        },
    ];

    // push task "a"
    tracker.push_tasks(vec![tasks[0].clone()]); // Topic 1

    // check topic state
    let topics = tracker.topics();
    assert!(topics.active.is_empty());
    assert_eq!(topics.pending.len(), 1);

    // pop task "a", making it active
    let popped = tracker.pop_tasks(10);
    assert_eq!(popped.len(), 1);

    // check topic state
    let topics = tracker.topics();
    assert_eq!(topics.active.len(), 1);
    assert!(topics.pending.is_empty());

    // mark task "a" as done
    tracker.task_done(&popped[0]);

    // check topic state
    let topics = tracker.topics();
    assert!(topics.pending.is_empty());
    assert!(topics.pending.is_empty());

    // push task "b"
    tracker.push_tasks(vec![tasks[1].clone()]);

    // check topic state
    let topics = tracker.topics();
    assert!(topics.active.is_empty());
    assert_eq!(topics.pending.len(), 1);

    // pop all tasks, "a" was done, "b" should have been allowed to be added
    let popped = tracker.pop_tasks(100);
    assert_eq!(popped.len(), 1);

    // check topic state
    let topics = tracker.topics();
    assert_eq!(topics.active.len(), 1);
    assert!(topics.pending.is_empty());
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
struct PermissiveTaskMerger {}

impl<T: Topic, D: Data> TaskMerger<T, D> for PermissiveTaskMerger {
    fn has_new_info(&self, _task_info: &Task<T, D>, _existing_tasks: &[Task<T, D>]) -> bool {
        true
    }

    fn merge(&self, task: &Task<T, D>, existing: &mut Task<T, D>) {
        existing.data = task.data.clone();
        existing.work = task.work;
    }
}
#[test]
fn test_replace_task_permissive() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, _, _>::new(
        partner,
        PermissiveTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: "a",
        },
        Task {
            topic: 1,
            priority: 20,
            work: 10,
            data: "b",
        },
    ];

    // push task "a"
    tracker.push_tasks(vec![tasks[0].clone()]); // Topic 1

    // push task "b", should replace "a"
    tracker.push_tasks(vec![tasks[1].clone()]); // Topic 1

    let popped = tracker.pop_tasks(100);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].data, "b");
    assert_eq!(popped[0].priority, 20);
}

#[test]
fn test_replace_task_size() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, _, _>::new(
        partner,
        PermissiveTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: "a",
        },
        Task {
            topic: 1,
            priority: 20,
            work: 20,
            data: "b",
        },
        Task {
            topic: 2,
            priority: 5,
            work: 5,
            data: "c",
        },
    ];

    tracker.push_tasks(vec![tasks[0].clone()]);
    // same topic, should replace "a" and update work from 10 to 20
    tracker.push_tasks(vec![tasks[1].clone()]);
    tracker.push_tasks(vec![tasks[2].clone()]);

    let popped = tracker.pop_tasks(15);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].data, "b");
    assert_eq!(tracker.get_pending_work(), 5);

    let popped = tracker.pop_tasks(30);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].data, "c");
    assert_eq!(tracker.get_pending_work(), 0);
}

#[test]
fn test_replace_active_task() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, _, _>::new(
        partner,
        PermissiveTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: "a",
        },
        Task {
            topic: 1,
            priority: 20,
            work: 10,
            data: "b",
        },
    ];

    tracker.push_tasks(vec![tasks[0].clone()]);
    // make "a" active
    let popped = tracker.pop_tasks(10);
    assert_eq!(popped.len(), 1);

    let a = &popped[0];

    // push "b"
    tracker.push_tasks(vec![tasks[1].clone()]);

    let popped = tracker.pop_tasks(100);
    assert_eq!(popped.len(), 1);

    let b = &popped[0];

    // finish tasks
    assert!(!tracker.is_idle());
    tracker.task_done(a);
    assert!(!tracker.is_idle());
    tracker.task_done(b);
    assert!(tracker.is_idle());
}

#[test]
fn test_replace_active_task_non_permissive() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, _, _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: "a",
        },
        Task {
            topic: 1,
            priority: 20,
            work: 10,
            data: "b",
        },
    ];

    tracker.push_tasks(vec![tasks[0].clone()]);
    let popped = tracker.pop_tasks(10);
    assert_eq!(popped.len(), 1);

    // non permissive merger should ignore this new t ask
    tracker.push_tasks(vec![tasks[1].clone()]);
    let popped = tracker.pop_tasks(100);
    assert!(popped.is_empty());
}

#[test]
fn test_replace_task_active_and_pending() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, _, _>::new(
        partner,
        PermissiveTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: "a",
        },
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: "b",
        },
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: "c",
        },
    ];

    tracker.push_tasks(vec![tasks[0].clone()]);
    let popped = tracker.pop_tasks(10);
    assert_eq!(popped.len(), 1);

    // "b" some topic, should be added to pending
    tracker.push_tasks(vec![tasks[1].clone()]);

    // "c", permissive should replace "b"
    tracker.push_tasks(vec![tasks[2].clone()]);

    let popped = tracker.pop_tasks(10);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].data, "c");
}

#[test]
fn test_remove_active() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, _, _>::new(
        partner,
        PermissiveTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 10,
            data: "a",
        },
        Task {
            topic: 1,
            priority: 20,
            work: 10,
            data: "b",
        },
        Task {
            topic: 2,
            priority: 15,
            work: 10,
            data: "c",
        },
    ];

    tracker.push_tasks(vec![tasks[0].clone()]);
    let popped = tracker.pop_tasks(10);
    assert_eq!(popped.len(), 1);

    // "b" and "c"
    tracker.push_tasks(vec![tasks[1].clone()]);
    tracker.push_tasks(vec![tasks[2].clone()]);

    // remove all topic 1
    tracker.remove(&1);
    let popped = tracker.pop_tasks(100);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].topic, 2);
}

#[test]
fn test_push_pop_equal_priorities() {
    let partner = PeerId::random();
    let mut tracker = PeerTracker::<usize, _, _>::new(
        partner,
        DefaultTaskMerger::default(),
        MAX_ACTIVE_WORK_PER_PEER,
    );

    let tasks = vec![
        Task {
            topic: 1,
            priority: 10,
            work: 1,
            data: (),
        },
        Task {
            topic: 2,
            priority: 10,
            work: 1,
            data: (),
        },
        Task {
            topic: 3,
            priority: 10,
            work: 1,
            data: (),
        },
    ];

    tracker.push_tasks(vec![tasks[0].clone()]);
    wait_for_new_instant();
    tracker.push_tasks(vec![tasks[1].clone()]);
    wait_for_new_instant();
    tracker.push_tasks(vec![tasks[2].clone()]);

    let popped = tracker.pop_tasks(1);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].topic, 1);

    let popped = tracker.pop_tasks(1);
    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].topic, 2);
    let popped = tracker.pop_tasks(1);

    assert_eq!(popped.len(), 1);
    assert_eq!(popped[0].topic, 3);
}

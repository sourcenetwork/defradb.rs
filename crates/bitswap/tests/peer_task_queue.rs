use std::fmt::Debug;

use bitswap::server::peer_task_queue::{
    Config, Data, DefaultTaskMerger, PeerTaskQueue, Task, TaskMerger, Topic,
};
use libp2p::PeerId;

#[test]
fn test_push_pop() {
    let mut ptq = PeerTaskQueue::<_, _, DefaultTaskMerger>::default();
    let partner = PeerId::random();
    let mut alphabet: Vec<char> = "abcdefghijklmnopqrstuvwxyz".chars().collect();
    let mut vowels: Vec<char> = "aeiou".chars().collect();
    let mut consonants: Vec<char> = alphabet
        .iter()
        .filter(|c| !vowels.contains(c))
        .copied()
        .collect();
    alphabet.sort();
    vowels.sort();
    consonants.sort();

    // Add blocks, cancel some, drain the queue.
    // The queue should only have the kept tasks at the end.

    let mut shuffled_alphabet = alphabet.clone();
    shuffled_alphabet.reverse();
    shuffled_alphabet.rotate_left(7);
    // add blocks for all letters
    for letter in shuffled_alphabet {
        let i = alphabet.iter().position(|c| *c == letter).unwrap();
        ptq.push_task(
            partner,
            Task {
                topic: letter,
                priority: i32::MAX as isize - i as isize,
                work: 0,
                data: (),
            },
        );
    }

    for consonant in &consonants {
        ptq.remove(consonant, partner);
    }

    ptq.full_thaw();

    let mut out = Vec::new();
    while let Some((_, received, _)) = ptq.pop_tasks(100) {
        if received.is_empty() {
            break;
        }
        for task in received {
            out.push(task.topic);
        }
    }

    assert_eq!(out.len(), vowels.len());

    // should be in correct order
    for (i, expected) in vowels.into_iter().enumerate() {
        assert_eq!(out[i], expected);
    }
}

#[test]
fn test_freeze_unfreeze() {
    let mut ptq = PeerTaskQueue::<_, _, DefaultTaskMerger>::default();
    let a = PeerId::random();
    let b = PeerId::random();
    let c = PeerId::random();
    let d = PeerId::random();

    for i in 0..5 {
        let task = Task {
            topic: i,
            work: 1,
            priority: 0,
            data: (),
        };

        ptq.push_task(a, task.clone());
        ptq.push_task(b, task.clone());
        ptq.push_task(c, task.clone());
        ptq.push_task(d, task);
    }

    println!("all four");
    match_n_tasks(&mut ptq, 4, &[a, b, c, d][..]);
    ptq.remove(&1, b);

    // b should be frozen
    println!("frozen b");
    match_n_tasks(&mut ptq, 3, &[a, c, d][..]);

    ptq.thaw_round();

    println!("unfrozen b");
    match_n_tasks(&mut ptq, 1, &[b][..]);

    // remove non existent task
    ptq.remove(&9, b);

    // b should not be frozen
    println!("all four again");
    match_n_tasks(&mut ptq, 4, &[a, b, c, d][..]);
}

#[test]
fn test_freeze_unfreeze_no_freezing() {
    let config = Config {
        ignore_freezing: true,
        ..Default::default()
    };
    let mut ptq =
        PeerTaskQueue::<_, _, DefaultTaskMerger>::new(DefaultTaskMerger::default(), config);
    let a = PeerId::random();
    let b = PeerId::random();
    let c = PeerId::random();
    let d = PeerId::random();

    for i in 0..5 {
        let task = Task {
            topic: i,
            work: 1,
            priority: 0,
            data: (),
        };

        ptq.push_task(a, task.clone());
        ptq.push_task(b, task.clone());
        ptq.push_task(c, task.clone());
        ptq.push_task(d, task);
    }

    match_n_tasks(&mut ptq, 4, &[a, b, c, d][..]);
    ptq.remove(&1, b);

    // b should not be frozen
    match_n_tasks(&mut ptq, 4, &[a, b, c, d][..]);
}

#[test]
fn test_peer_order() {
    let mut ptq = PeerTaskQueue::<_, _, DefaultTaskMerger>::default();
    let a = PeerId::random();
    let b = PeerId::random();
    let c = PeerId::random();

    ptq.push_task(
        a,
        Task {
            topic: 1,
            work: 3,
            priority: 2,
            data: (),
        },
    );
    ptq.push_task(
        a,
        Task {
            topic: 2,
            work: 1,
            priority: 1,
            data: (),
        },
    );

    ptq.push_task(
        b,
        Task {
            topic: 3,
            work: 1,
            priority: 3,
            data: (),
        },
    );
    ptq.push_task(
        b,
        Task {
            topic: 4,
            work: 3,
            priority: 2,
            data: (),
        },
    );
    ptq.push_task(
        b,
        Task {
            topic: 5,
            work: 1,
            priority: 1,
            data: (),
        },
    );

    ptq.push_task(
        c,
        Task {
            topic: 6,
            work: 2,
            priority: 2,
            data: (),
        },
    );
    ptq.push_task(
        c,
        Task {
            topic: 7,
            work: 2,
            priority: 1,
            data: (),
        },
    );

    // all peers have nothing in their active so equal of any peer being chosen

    let mut peers = Vec::new();
    let mut ids = Vec::new();
    for _i in 0..3 {
        let (peer, tasks, _) = ptq.pop_tasks(1).unwrap();
        peers.push(peer);
        assert_eq!(tasks.len(), 1);
        ids.push(tasks[0].topic);
    }

    assert_eq_unordered(peers, [a, b, c]);
    assert_eq_unordered(ids, [1, 3, 6]);

    // Active queues:
    // a: 3            Pending: [1]
    // b: 1            Pending: [3, 1]
    // c: 2            Pending: [2]
    // So next peer should be b (least work in active queue)
    let (peer, task, pending) = ptq.pop_tasks(1).unwrap();
    assert_eq!(task.len(), 1);
    assert_eq!(peer, b);
    assert_eq!(task[0].topic, 4);
    assert_eq!(pending, 1);

    // Active queues:
    // a: 3            Pending: [1]
    // b: 1 + 3        Pending: [1]
    // c: 2            Pending: [2]
    // So next peer should be c (least work in active queue)
    let (peer, task, _) = ptq.pop_tasks(1).unwrap();
    assert_eq!(task.len(), 1);
    assert_eq!(peer, c);
    assert_eq!(task[0].topic, 7);

    // Active queues:
    // a: 3            Pending: [1]
    // b: 1 + 3        Pending: [1]
    // c: 2 + 2
    // So next peer should be a (least work in active queue)
    let (peer, task, pending) = ptq.pop_tasks(1).unwrap();
    assert_eq!(task.len(), 1);
    assert_eq!(peer, a);
    assert_eq!(task[0].topic, 2);
    assert_eq!(pending, 0);

    // Active queues:
    // a: 3 + 1
    // b: 1 + 3        Pending: [1]
    // c: 2 + 2
    // a & c have no more pending tasks, so next peer should be b
    let (peer, task, pending) = ptq.pop_tasks(1).unwrap();
    assert_eq!(task.len(), 1);
    assert_eq!(peer, b);
    assert_eq!(task[0].topic, 5);
    assert_eq!(pending, 0);

    // Active queues:
    // a: 3 + 1
    // b: 1 + 3 + 1
    // c: 2 + 2
    // No more pending tasks, so next pop should return nothing
    let (_peer, task, pending) = ptq.pop_tasks(1).unwrap();
    assert!(task.is_empty());
    assert_eq!(pending, 0);
}

#[test]
fn test_cleaning_up() {
    let mut ptq = PeerTaskQueue::<_, _, DefaultTaskMerger>::default();
    let peer = PeerId::random();

    let peer_tasks: Vec<_> = (0..5)
        .map(|i| Task {
            topic: i,
            priority: 0,
            work: 0,
            data: (),
        })
        .collect();
    // push a block, pop a block,  complete eerything, should be removed

    ptq.push_tasks(peer, peer_tasks.clone());
    let (peer, tasks, _) = ptq.pop_tasks(100).unwrap();
    ptq.tasks_done(peer, &tasks);
    let (_, tasks, _) = ptq.pop_tasks(100).unwrap();
    assert!(tasks.is_empty());
    assert_eq!(ptq.stats().num_peers, 0);
    // push a block, remove each of its entries, should be removed
    ptq.push_tasks(peer, peer_tasks.clone());
    for task in peer_tasks {
        ptq.remove(&task.topic, peer);
    }
    let (_, tasks, _) = ptq.pop_tasks(100).unwrap();
    assert!(tasks.is_empty());
    assert_eq!(ptq.stats().num_peers, 0);
}

fn match_n_tasks<T: Topic, D: Data, TM: TaskMerger<T, D>>(
    ptq: &mut PeerTaskQueue<T, D, TM>,
    n: usize,
    expected: &[PeerId],
) {
    let mut targets = Vec::new();
    for i in 0..n {
        let (peer, tasks, _) = ptq.pop_tasks(1).unwrap();
        assert_eq!(tasks.len(), 1, "task {i} did not match: {tasks:?}");
        targets.push(peer);
    }
    assert_eq_unordered(expected, targets);
}

fn assert_eq_unordered<T: Ord + Eq + Debug + Clone>(a: impl AsRef<[T]>, b: impl AsRef<[T]>) {
    let mut a: Vec<_> = a.as_ref().iter().collect();
    a.sort();
    let mut b: Vec<_> = b.as_ref().iter().collect();
    b.sort();
    assert_eq!(a, b);
}

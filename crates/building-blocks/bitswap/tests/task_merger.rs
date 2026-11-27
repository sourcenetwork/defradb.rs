use bitswap::server::peer_task_queue::{Task, TaskMerger as _};
use bitswap::server::task_merger::{TaskData, TaskMerger};

mod common;

fn task(
    seed: &[u8],
    is_want_block: bool,
    have_block: bool,
    block_size: usize,
    work: usize,
) -> Task<cid::Cid, TaskData> {
    Task {
        topic: common::cid_v1(seed),
        priority: 1,
        work,
        data: TaskData {
            is_want_block,
            send_dont_have: true,
            block_size,
            have_block,
        },
    }
}

#[test]
fn want_block_is_new_info_over_want_have() {
    let existing = [task(b"a", false, true, 10, 5)];
    assert!(TaskMerger::default().has_new_info(&task(b"a", true, true, 10, 10), &existing));
}

#[test]
fn size_is_new_info_over_dont_have() {
    let existing = [task(b"a", false, false, 0, 5)];
    assert!(TaskMerger::default().has_new_info(&task(b"a", false, true, 10, 5), &existing));
}

#[test]
fn repeat_adds_nothing() {
    let existing = [task(b"a", true, true, 10, 10)];
    assert!(!TaskMerger::default().has_new_info(&task(b"a", true, true, 10, 10), &existing));
    assert!(!TaskMerger::default().has_new_info(&task(b"a", false, true, 10, 5), &existing));
}

#[test]
fn merge_upgrades_have_to_block_with_block_size_work() {
    let mut existing = task(b"a", false, true, 4000, 40);
    TaskMerger::default().merge(&task(b"a", true, true, 4000, 4000), &mut existing);
    assert!(existing.data.is_want_block);
    assert!(existing.data.have_block);
    assert_eq!(existing.work, 4000);
}

#[test]
fn merge_adopts_size_for_dont_have() {
    let mut existing = task(b"a", false, false, 0, 40);
    TaskMerger::default().merge(&task(b"a", false, true, 4000, 40), &mut existing);
    assert!(existing.data.have_block);
    assert_eq!(existing.data.block_size, 4000);
    assert!(!existing.data.is_want_block);
    assert_eq!(existing.work, 40);
}

#[test]
fn merge_want_block_without_size_over_dont_have_keeps_it_unsized() {
    let mut existing = task(b"a", false, false, 0, 40);
    TaskMerger::default().merge(&task(b"a", true, false, 0, 40), &mut existing);
    assert!(existing.data.is_want_block);
    assert!(!existing.data.have_block);
    assert_eq!(existing.work, 40);
}

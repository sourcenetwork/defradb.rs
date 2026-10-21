//! Head sets: the per-name function algebra, its laws against the engine's
//! contract, and the out-of-order cases it exists for.

use regolith::testing::proptest::prelude::*;
use regolith::MergeOperator;

use crate::corekv::Store;
use crate::merge::head_set::part;
use crate::merge::head_set::{Set, Write};
use crate::merge::DefraMergeOperator;
use crate::RegolithStore;

fn fold(base: Option<&[u8]>, writes: &[&Write]) -> Set {
    let operands: Vec<Vec<u8>> = writes.iter().map(|write| write.operand()).collect();
    let refs: Vec<&[u8]> = operands.iter().map(Vec::as_slice).collect();
    let bytes = DefraMergeOperator
        .full_merge(b"set", base, &refs)
        .expect("a head set operand folds");
    Set::read(&bytes).expect("a folded head set reads back")
}

fn live(set: &Set) -> Vec<(Vec<u8>, u64)> {
    set.live()
        .map(|(name, priority)| (name.to_vec(), priority))
        .collect()
}

fn adds(names: &[(&[u8], u64)]) -> Write {
    let mut write = Write::default();
    for (name, priority) in names {
        write.add(name, *priority);
    }
    write
}

/// A write of one block: its own head, and each parent it supersedes.
fn block(parents: &[&[u8]], ahead: &[&[u8]], head: (&[u8], u64)) -> Write {
    let mut write = Write::default();
    for parent in parents {
        write.supersede(parent);
    }
    for parent in ahead {
        write.supersede_ahead(parent);
    }
    write.add(head.0, head.1);
    write
}

fn names() -> impl Strategy<Value = Vec<u8>> + Clone {
    prop_oneof![
        Just(b"a".to_vec()),
        Just(b"b".to_vec()),
        Just(b"c".to_vec()),
        Just(b"field/cid-x".to_vec()),
    ]
}

/// A write with every entry kind on names that collide.
fn writes() -> impl Strategy<Value = Write> + Clone {
    proptest::collection::vec((names(), 0u8..3, any::<u64>()), 1..5).prop_map(|entries| {
        let mut write = Write::default();
        for (name, kind, priority) in entries {
            match kind {
                0 => write.add(&name, priority),
                1 => write.supersede(&name),
                _ => write.supersede_ahead(&name),
            }
        }
        write
    })
}

fn operands() -> impl Strategy<Value = Vec<u8>> + Clone {
    writes()
        .prop_filter("a write that changes nothing has no operand", |write| {
            !write.is_empty()
        })
        .prop_map(|write| write.operand())
}

fn bases() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec((names(), any::<bool>(), any::<u64>()), 0..4).prop_map(|names| {
        let mut write = Write::default();
        for (name, is_live, priority) in names {
            if is_live {
                write.add(&name, priority);
            } else {
                write.supersede_ahead(&name);
            }
        }
        DefraMergeOperator
            .full_merge(b"set", None, &[&write.operand()])
            .unwrap()
    })
}

#[test]
fn head_set_folds_obey_the_engine_contract() {
    let checked = regolith::testing::check_merge_operator(
        &DefraMergeOperator,
        Just(b"set".to_vec()),
        bases(),
        operands(),
    )
    .unwrap();
    assert!(checked.folds > 0);
}

#[test]
fn head_set_touches_obey_the_engine_contract() {
    let checked = regolith::testing::check_touches(
        &DefraMergeOperator,
        Just(b"set".to_vec()),
        operands(),
        proptest::collection::vec(0u32..4, 0..3),
    )
    .unwrap();
    assert!(checked.folds > 0);
}

/// Every operand touches every part: a head set is read without parts.
#[test]
fn every_head_set_operand_touches_every_part() {
    let operand = adds(&[(b"a", 1)]).operand();
    for parts in [&[][..], &[1], &[1, 2]] {
        assert!(DefraMergeOperator.touches(b"set", &operand, parts));
    }
}

#[test]
fn an_add_makes_a_head_and_a_supersede_ends_it() {
    let set = fold(None, &[&adds(&[(b"a", 3), (b"b", 5)])]);
    assert_eq!(live(&set), [(b"a".to_vec(), 3), (b"b".to_vec(), 5)]);
    let set = fold(
        None,
        &[
            &adds(&[(b"a", 3), (b"b", 5)]),
            &block(&[b"a"], &[], (b"c", 6)),
        ],
    );
    assert_eq!(live(&set), [(b"b".to_vec(), 5), (b"c".to_vec(), 6)]);
    assert_eq!(set.ahead().count(), 0);
}

#[test]
fn a_repeated_add_keeps_the_first_priority() {
    let set = fold(None, &[&adds(&[(b"a", 3)]), &adds(&[(b"a", 9)])]);
    assert_eq!(live(&set), [(b"a".to_vec(), 3)]);
}

/// The child's block arrives before its parent's: the parent is held ahead,
/// and its own add consumes that, leaving only the child live.
#[test]
fn a_child_before_its_parent_leaves_the_child_live_and_no_ahead_name() {
    let child = block(&[], &[b"p"], (b"c", 2));
    let parent = adds(&[(b"p", 1)]);
    let after_child = fold(None, &[&child]);
    assert_eq!(live(&after_child), [(b"c".to_vec(), 2)]);
    assert_eq!(after_child.ahead().collect::<Vec<_>>(), [b"p".as_slice()]);
    let set = fold(None, &[&child, &parent]);
    assert_eq!(live(&set), [(b"c".to_vec(), 2)]);
    assert_eq!(set.ahead().count(), 0);
    assert!(!set.is_live(b"p"));
}

/// A writer supersedes ahead only while the parent is unpublished in its
/// validated read, so children that arrive before the parent supersede ahead
/// and children written after it supersede.
#[test]
fn two_children_of_one_parent_leave_both_live_after_the_parent() {
    let parent = adds(&[(b"p", 1)]);
    let early = [
        block(&[], &[b"p"], (b"c1", 2)),
        block(&[], &[b"p"], (b"c2", 2)),
    ];
    let late = [
        block(&[b"p"], &[], (b"c1", 2)),
        block(&[b"p"], &[], (b"c2", 2)),
    ];
    for order in [
        [&early[0], &early[1], &parent],
        [&parent, &late[0], &late[1]],
        [&early[0], &parent, &late[1]],
    ] {
        let set = fold(None, &order);
        let heads: Vec<_> = live(&set).into_iter().map(|(name, _)| name).collect();
        assert_eq!(heads, [b"c1".to_vec(), b"c2".to_vec()]);
        assert_eq!(set.ahead().count(), 0);
    }
}

/// A parent already published, whose head was since superseded: a late
/// sibling's supersede of it changes nothing.
#[test]
fn a_supersede_of_a_reclaimed_parent_changes_nothing() {
    let set = fold(
        None,
        &[
            &adds(&[(b"p", 1)]),
            &block(&[b"p"], &[], (b"c1", 2)),
            &block(&[b"p"], &[], (b"c2", 2)),
        ],
    );
    assert_eq!(live(&set).len(), 2);
    assert_eq!(set.ahead().count(), 0);
}

#[test]
fn a_fork_merges_into_one_head() {
    let set = fold(
        None,
        &[
            &adds(&[(b"p", 1)]),
            &block(&[b"p"], &[], (b"x", 2)),
            &block(&[b"p"], &[], (b"y", 2)),
            &block(&[b"x", b"y"], &[], (b"m", 3)),
        ],
    );
    assert_eq!(live(&set), [(b"m".to_vec(), 3)]);
}

#[test]
fn malformed_bytes_fail_with_none() {
    let operand = block(&[b"a"], &[b"b"], (b"c", 7)).operand();
    for end in 0..operand.len() {
        assert!(
            DefraMergeOperator
                .full_merge(b"set", None, &[&operand[..end]])
                .is_none(),
            "an operand cut at {end}"
        );
    }
    let mut longer = operand.clone();
    longer.push(0);
    assert!(DefraMergeOperator
        .full_merge(b"set", None, &[&longer])
        .is_none());
    let value = DefraMergeOperator
        .full_merge(b"set", None, &[&operand])
        .unwrap();
    // A value is no operand and an operand is no value.
    assert!(DefraMergeOperator
        .full_merge(b"set", None, &[&value])
        .is_none());
    assert!(DefraMergeOperator
        .full_merge(b"set", Some(&operand), &[&operand])
        .is_none());
    assert!(Set::read(&operand).is_none());
    for end in 0..value.len() {
        assert!(Set::read(&value[..end]).is_none(), "a value cut at {end}");
    }
    assert!(DefraMergeOperator
        .partial_merge(b"set", &operand, &value)
        .is_none());
    // Names must ascend and never repeat.
    let mut duplicate = Vec::new();
    for _ in 0..2 {
        duplicate.extend_from_slice(&[1, b'a', 3, 1, 0, 0]);
    }
    let body = [&[2u8][..], &duplicate].concat();
    let forged = [b"DM\x01\x08".as_slice(), &body].concat();
    assert!(DefraMergeOperator
        .full_merge(b"set", None, &[&forged])
        .is_none());
}

#[test]
fn an_empty_write_has_no_operand_to_merge() {
    let mut write = Write::default();
    assert!(write.is_empty());
    write.supersede(b"a");
    assert!(!write.is_empty());
}

/// Two transactions that both read the set and merge into it commit: the
/// merge is blind, and the live set is the union of what both added.
#[test]
fn concurrent_writers_of_one_set_both_commit() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut seed = store.new_txn(false).await.unwrap();
        seed.merge(b"h/hd/set", &adds(&[(b"p", 1)]).operand())
            .await
            .unwrap();
        seed.commit().await.unwrap();
        let mut first = store.new_txn(false).await.unwrap();
        let mut second = store.new_txn(false).await.unwrap();
        for (txn, head) in [(&mut first, b"x"), (&mut second, b"y")] {
            let read = txn.get_parts(b"h/hd/set", &[]).await.unwrap().unwrap();
            assert!(Set::read(&read).unwrap().is_live(b"p"));
            txn.merge(b"h/hd/set", &block(&[b"p"], &[], (head, 2)).operand())
                .await
                .unwrap();
        }
        first.commit().await.unwrap();
        second.commit().await.unwrap();
        let read = store.new_txn(true).await.unwrap();
        let set = Set::read(&read.get(b"h/hd/set").await.unwrap().unwrap()).unwrap();
        assert_eq!(
            live(&set),
            [(b"x".to_vec(), 2), (b"y".to_vec(), 2)],
            "both heads are live and the parent is gone"
        );
    })
    .unwrap()
}

/// A transaction reads its own pending merges.
#[test]
fn a_transaction_reads_the_set_it_merged_into() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        txn.merge(b"h/hd/set", &adds(&[(b"a", 1)]).operand())
            .await
            .unwrap();
        txn.merge(b"h/hd/set", &block(&[b"a"], &[], (b"b", 2)).operand())
            .await
            .unwrap();
        let read = txn.get_parts(b"h/hd/set", &[]).await.unwrap().unwrap();
        assert_eq!(live(&Set::read(&read).unwrap()), [(b"b".to_vec(), 2)]);
    })
    .unwrap()
}

/// An operand touches the part of every name it holds and of no other name;
/// an id that is no name's part is touched by every operand.
#[test]
fn an_operand_touches_the_parts_of_the_names_it_holds() {
    let operand = block(&[b"p"], &[b"q"], (b"c", 2)).operand();
    let held = [b"p".as_slice(), b"q", b"c"].map(part);
    for name in held {
        assert!(DefraMergeOperator.touches(b"set", &operand, &[name]));
    }
    let other = part(b"other");
    assert!(!held.contains(&other), "these names hash to distinct parts");
    assert!(!DefraMergeOperator.touches(b"set", &operand, &[other]));
    assert!(DefraMergeOperator.touches(b"set", &operand, &[7, other]));
}

/// The law, with the parts of names: what a fold touches is what its two
/// operands touched.
#[test]
fn head_set_touches_by_name_obey_the_engine_contract() {
    let parts = [b"a".as_slice(), b"b", b"c", b"field/cid-x", b"unwritten"]
        .map(part)
        .to_vec();
    let checked = regolith::testing::check_touches(
        &DefraMergeOperator,
        Just(b"set".to_vec()),
        operands(),
        proptest::sample::subsequence(parts, 1..=3),
    )
    .unwrap();
    assert!(checked.folds > 0);
}

/// A transaction that read one name's part commits past a merge that holds
/// other names only, and is refused by one that holds its name.
#[test]
fn a_read_of_one_names_part_meets_only_operands_that_hold_it() {
    workers::block_on(async {
        let theirs = [
            (adds(&[(b"x", 1)]), false),
            (adds(&[(b"p", 1)]), true),
            (block(&[], &[b"p"], (b"y", 2)), true),
        ];
        for (write, refused) in theirs {
            let store = RegolithStore::in_memory().unwrap();
            let mut seed = store.new_txn(false).await.unwrap();
            seed.merge(b"h/hd/set", &adds(&[(b"s", 1)]).operand())
                .await
                .unwrap();
            seed.commit().await.unwrap();
            let mut reader = store.new_txn(false).await.unwrap();
            reader.get_parts(b"h/hd/set", &[part(b"p")]).await.unwrap();
            reader
                .merge(b"h/hd/set", &block(&[], &[b"p"], (b"c", 2)).operand())
                .await
                .unwrap();
            let mut writer = store.new_txn(false).await.unwrap();
            writer.merge(b"h/hd/set", &write.operand()).await.unwrap();
            writer.commit().await.unwrap();
            assert_eq!(reader.commit().await.is_err(), refused, "{write:?}");
        }
    })
    .unwrap()
}

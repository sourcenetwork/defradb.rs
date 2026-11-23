//! Owed ACP state: four counters, each one part, against the engine's
//! contract, and the reads it exists for.

use regolith::testing::proptest::prelude::*;
use regolith::MergeOperator;

use crate::corekv::{Reader, Store};
use crate::merge::owed::{delta, part, read, Counters};
use crate::merge::DefraMergeOperator;
use crate::RegolithStore;

/// What a stager adds: one change, a delete or not.
fn staged(delete: bool) -> Counters {
    [1, u64::from(delete), 0, 0]
}

/// What settling adds: one change, a delete or not.
fn settled(delete: bool) -> Counters {
    [0, 0, 1, u64::from(delete)]
}

fn amounts() -> impl Strategy<Value = Counters> + Clone {
    prop_oneof![
        any::<bool>().prop_map(staged),
        any::<bool>().prop_map(settled),
        proptest::array::uniform4(0u64..1_000),
    ]
}

fn operands() -> impl Strategy<Value = Vec<u8>> + Clone {
    amounts().prop_map(delta)
}

fn bases() -> impl Strategy<Value = Vec<u8>> {
    amounts().prop_map(|amounts| {
        DefraMergeOperator::default()
            .full_merge(b"owed", None, &[&delta(amounts)])
            .unwrap()
    })
}

#[test]
fn owed_folds_obey_the_engine_contract() {
    let checked = regolith::testing::check_merge_operator(
        &DefraMergeOperator::default(),
        Just(b"owed".to_vec()),
        bases(),
        operands(),
    )
    .unwrap();
    assert!(checked.folds > 0);
}

#[test]
fn owed_touches_obey_the_engine_contract() {
    let checked = regolith::testing::check_touches(
        &DefraMergeOperator::default(),
        Just(b"owed".to_vec()),
        operands(),
        proptest::collection::vec(0u32..5, 0..3),
    )
    .unwrap();
    assert!(checked.folds > 0);
}

/// Settling touches no staged counter and staging no settled one, so the
/// two never refuse each other's readers.
#[test]
fn staging_and_settling_touch_disjoint_parts() {
    let operator = DefraMergeOperator::default();
    let staged_part = [part(0), part(1)];
    let settled_part = [part(2), part(3)];
    for delete in [false, true] {
        assert!(!operator.touches(b"owed", &delta(settled(delete)), &staged_part));
        assert!(operator.touches(b"owed", &delta(settled(delete)), &settled_part));
        assert!(operator.touches(b"owed", &delta(staged(delete)), &staged_part));
        assert!(!operator.touches(b"owed", &delta(staged(delete)), &settled_part));
    }
    // A grant touches the staged changes and not the staged deletes, so a
    // denial decided on deletes alone is not refused for it.
    assert!(!operator.touches(b"owed", &delta(staged(false)), &[part(1)]));
    assert!(operator.touches(b"owed", &delta(staged(true)), &[part(1)]));
}

#[test]
fn an_absent_value_is_zero_and_an_overflow_fails_the_merge() {
    let operator = DefraMergeOperator::default();
    let value = operator
        .full_merge(
            b"owed",
            None,
            &[&delta(staged(true)), &delta(settled(true))],
        )
        .unwrap();
    assert_eq!(read(&value), Some([1, 1, 1, 1]));
    let full = operator
        .full_merge(b"owed", None, &[&delta([u64::MAX, 0, 0, 0])])
        .unwrap();
    assert!(operator
        .full_merge(b"owed", Some(&full), &[&delta(staged(false))])
        .is_none());
    assert!(read(b"short").is_none());
}

/// A transaction that read the staged part commits past a settle, and is
/// refused by a stage: what a stager and a deny decision rely on.
#[test]
fn a_read_of_the_staged_part_meets_only_staging() {
    workers::block_on(async {
        let theirs = [
            (settled(true), false),
            (staged(false), true),
            (staged(true), true),
        ];
        for (amounts, refused) in theirs {
            let store = RegolithStore::in_memory().unwrap();
            let mut seed = store.new_txn(false).await.unwrap();
            seed.merge(b"o/owed", &delta(staged(true))).await.unwrap();
            seed.commit().await.unwrap();
            let mut reader = store.new_txn(false).await.unwrap();
            reader
                .get_parts(b"o/owed", &[part(0), part(1)])
                .await
                .unwrap();
            reader
                .merge(b"o/owed", &delta(staged(false)))
                .await
                .unwrap();
            let mut writer = store.new_txn(false).await.unwrap();
            writer.merge(b"o/owed", &delta(amounts)).await.unwrap();
            writer.commit().await.unwrap();
            assert_eq!(reader.commit().await.is_err(), refused, "{amounts:?}");
        }
    })
    .unwrap()
}

/// A fully settled value owes nothing whether or not compaction has folded
/// it, and compaction keeps it: an operand-only chain is never shown to a
/// compaction filter, so it stays as one folded operand.
#[test]
fn a_fully_settled_value_owes_nothing_before_and_after_compaction() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        for amounts in [staged(true), staged(false), settled(true), settled(false)] {
            let mut txn = store.new_txn(false).await.unwrap();
            txn.merge(b"o/owed", &delta(amounts)).await.unwrap();
            txn.commit().await.unwrap();
        }
        let owed = |counters: Counters| {
            (
                counters[0].saturating_sub(counters[2]),
                counters[1].saturating_sub(counters[3]),
            )
        };
        let value = || async {
            let reader = store.new_txn(true).await.unwrap();
            let bytes = reader.get(b"o/owed").await.unwrap().expect("the value");
            bytes.to_vec()
        };
        let before = value().await;
        assert_eq!(read(&before), Some([2, 1, 2, 1]));
        assert_eq!(owed(read(&before).unwrap()), (0, 0));

        store.engine().db().flush().unwrap();
        store
            .engine()
            .db()
            .compact_range(None, None)
            .wait()
            .unwrap();
        let after = value().await;
        assert_eq!(read(&after), read(&before));
        assert_eq!(owed(read(&after).unwrap()), (0, 0));
    })
    .unwrap()
}

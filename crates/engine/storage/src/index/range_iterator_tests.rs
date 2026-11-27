use super::*;
use crate::backends::RegolithStore;
use crate::corekv::Store;
use crate::index::{CollectionIndex, SimpleIndex};
use schema::IndexedFieldDescription;

fn test_index_description() -> IndexDescription {
    IndexDescription {
        id: 1,
        name: "test_index".to_string(),
        unique: false,
        kind: None,
        auto_generated: false,
        fields: vec![IndexedFieldDescription {
            name: "age".to_string(),
            descending: false,
        }],
    }
}

fn composite_index_description() -> IndexDescription {
    IndexDescription {
        id: 2,
        name: "composite_index".to_string(),
        unique: false,
        kind: None,
        auto_generated: false,
        fields: vec![
            IndexedFieldDescription {
                name: "category".to_string(),
                descending: false,
            },
            IndexedFieldDescription {
                name: "score".to_string(),
                descending: false,
            },
        ],
    }
}

#[test]
fn test_range_scan_all() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();

        let desc = test_index_description();
        let index = SimpleIndex::new(1, desc.clone());

        index
            .save(&mut txn, 1, &[NormalValue::Int(10)])
            .await
            .unwrap();
        index
            .save(&mut txn, 2, &[NormalValue::Int(20)])
            .await
            .unwrap();
        index
            .save(&mut txn, 3, &[NormalValue::Int(30)])
            .await
            .unwrap();
        txn.commit().await.unwrap();

        let txn = store.new_txn(true).await.unwrap();
        let mut iter = RangeIterator::new_scan(txn.as_ref(), 1, &desc, false, false)
            .await
            .unwrap();

        let entries = iter.collect_all().await.unwrap();
        assert_eq!(entries.len(), 3);
    })
    .unwrap()
}

#[test]
fn test_range_scan_reverse() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();

        let desc = test_index_description();
        let index = SimpleIndex::new(1, desc.clone());

        index
            .save(&mut txn, 1, &[NormalValue::Int(10)])
            .await
            .unwrap();
        index
            .save(&mut txn, 2, &[NormalValue::Int(20)])
            .await
            .unwrap();
        index
            .save(&mut txn, 3, &[NormalValue::Int(30)])
            .await
            .unwrap();
        txn.commit().await.unwrap();

        let txn = store.new_txn(true).await.unwrap();
        let mut iter = RangeIterator::new_scan(txn.as_ref(), 1, &desc, false, true)
            .await
            .unwrap();

        let entries = iter.collect_all().await.unwrap();
        assert_eq!(entries.len(), 3);

        // Should be in reverse order
        assert_eq!(entries[0].values[0], NormalValue::Int(30));
        assert_eq!(entries[2].values[0], NormalValue::Int(10));
    })
    .unwrap()
}

#[test]
fn test_range_prefix_scan() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();

        let desc = composite_index_description();
        let index = SimpleIndex::new(1, desc.clone());

        // Save documents with different categories
        index
            .save(
                &mut txn,
                1,
                &[NormalValue::String("A".to_string()), NormalValue::Int(100)],
            )
            .await
            .unwrap();
        index
            .save(
                &mut txn,
                2,
                &[NormalValue::String("A".to_string()), NormalValue::Int(200)],
            )
            .await
            .unwrap();
        index
            .save(
                &mut txn,
                3,
                &[NormalValue::String("B".to_string()), NormalValue::Int(150)],
            )
            .await
            .unwrap();
        txn.commit().await.unwrap();

        // Scan only category "A"
        let txn = store.new_txn(true).await.unwrap();
        let mut iter = RangeIterator::new_prefix(
            txn.as_ref(),
            1,
            &desc,
            false,
            &[NormalValue::String("A".to_string())],
            false,
        )
        .await
        .unwrap();

        let entries = iter.collect_all().await.unwrap();
        assert_eq!(entries.len(), 2);
    })
    .unwrap()
}

#[test]
fn test_range_bounded() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();

        let desc = test_index_description();
        let index = SimpleIndex::new(1, desc.clone());

        for i in 1..=10 {
            index
                .save(&mut txn, i as u64, &[NormalValue::Int(i * 10)])
                .await
                .unwrap();
        }
        txn.commit().await.unwrap();

        // Range: 30 < x <= 70
        let txn = store.new_txn(true).await.unwrap();
        let mut iter = RangeIterator::new_range(
            txn.as_ref(),
            1,
            &desc,
            false,
            &[],
            Bound::gt(NormalValue::Int(30)),
            Bound::le(NormalValue::Int(70)),
            false,
        )
        .await
        .unwrap();

        let entries = iter.collect_all().await.unwrap();
        // Should include 40, 50, 60, 70
        assert_eq!(entries.len(), 4);
    })
    .unwrap()
}

/// Build an encoded seek key for an integer value on the test index (collection=1, index=1).
fn build_age_seek_key(age: i64) -> Vec<u8> {
    let key_prefix = IndexDataStoreKey::index_prefix(1, 1);
    encode_field_value(key_prefix, &NormalValue::Int(age), false).unwrap()
}

#[test]
fn test_cursor_seek_forward_exclusive_skips_boundary() {
    workers::block_on(async {
        // Setup: age index with doc 1 = 20, doc 2 = 30, doc 3 = 40.
        // Forward exclusive seek at age=30 → should return only doc 3 (age=40).
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();

        let desc = test_index_description();
        let index = SimpleIndex::new(1, desc.clone());

        index
            .save(&mut txn, 1, &[NormalValue::Int(20)])
            .await
            .unwrap();
        index
            .save(&mut txn, 2, &[NormalValue::Int(30)])
            .await
            .unwrap();
        index
            .save(&mut txn, 3, &[NormalValue::Int(40)])
            .await
            .unwrap();
        txn.commit().await.unwrap();

        let txn = store.new_txn(true).await.unwrap();
        let mut iter = RangeIterator::new_scan(txn.as_ref(), 1, &desc, false, false)
            .await
            .unwrap();

        // Seek to age=30 exclusive (forward): iterator should land at doc 3 (age=40).
        let seek_key = build_age_seek_key(30);
        iter.apply_cursor_seek(seek_key, false, false)
            .await
            .unwrap();

        let entries = iter.collect_all().await.unwrap();
        let doc_ids: Vec<u64> = entries.iter().map(|e| e.doc_short_id).collect();

        assert!(!doc_ids.contains(&1), "doc 1 should be before the cursor");
        assert!(
            !doc_ids.contains(&2),
            "doc 2 is the exclusive boundary and must be skipped"
        );
        assert!(
            doc_ids.contains(&3),
            "doc 3 must be included (after boundary)"
        );
        assert_eq!(entries.len(), 1);
    })
    .unwrap()
}

#[test]
fn test_cursor_seek_backward_inclusive_starts_at_boundary() {
    workers::block_on(async {
        // Setup: age index with doc 1 = 20, doc 2 = 30, doc 3 = 40.
        // Backward inclusive seek at age=30 → should return doc 2 then doc 1.
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();

        let desc = test_index_description();
        let index = SimpleIndex::new(1, desc.clone());

        index
            .save(&mut txn, 1, &[NormalValue::Int(20)])
            .await
            .unwrap();
        index
            .save(&mut txn, 2, &[NormalValue::Int(30)])
            .await
            .unwrap();
        index
            .save(&mut txn, 3, &[NormalValue::Int(40)])
            .await
            .unwrap();
        txn.commit().await.unwrap();

        let txn = store.new_txn(true).await.unwrap();
        let mut iter = RangeIterator::new_scan(txn.as_ref(), 1, &desc, false, true)
            .await
            .unwrap();

        // Seek to age=30 inclusive (reverse): iterator should include docs 2 and 1.
        // The seek_key encodes age=30; for a reverse scan the storage seek lands at or before it.
        let seek_key = build_age_seek_key(30);
        iter.apply_cursor_seek(seek_key, true, true).await.unwrap();

        let entries = iter.collect_all().await.unwrap();
        let doc_ids: Vec<u64> = entries.iter().map(|e| e.doc_short_id).collect();

        assert!(
            !doc_ids.contains(&3),
            "doc 3 is after the cursor and must be excluded"
        );
        assert!(
            doc_ids.contains(&2),
            "doc 2 is the inclusive boundary and must be included"
        );
        assert!(
            doc_ids.contains(&1),
            "doc 1 must be included (before boundary in reverse)"
        );
        assert_eq!(entries.len(), 2);
    })
    .unwrap()
}

#[test]
fn test_cursor_seek_reversed_param_controls_upper_bound() {
    workers::block_on(async {
        // Verify that the `reversed` parameter controls which bound slot is used,
        // overriding `self.reverse`. This is the P1.1 regression guard: the
        // `reversed` field in CursorSeek must be propagated explicitly rather than
        // silently relying on `self.reverse` already being set correctly.
        //
        // Scenario: build a reverse iterator (self.reverse=true), then call
        // apply_cursor_seek with reversed=true at age=30 inclusive.
        // The upper_bound_key should be set (reverse path), not lower_bound_key.
        // The existing test_cursor_seek_backward_inclusive_starts_at_boundary already
        // validates the behavioral outcome; this test checks the bound slot directly.
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();

        let desc = test_index_description();
        let index = SimpleIndex::new(1, desc.clone());

        index
            .save(&mut txn, 1, &[NormalValue::Int(20)])
            .await
            .unwrap();
        index
            .save(&mut txn, 2, &[NormalValue::Int(30)])
            .await
            .unwrap();
        index
            .save(&mut txn, 3, &[NormalValue::Int(40)])
            .await
            .unwrap();
        txn.commit().await.unwrap();

        // Build a reverse iterator to match the reversed=true param.
        let txn = store.new_txn(true).await.unwrap();
        let mut iter = RangeIterator::new_scan(txn.as_ref(), 1, &desc, false, true)
            .await
            .unwrap();

        let seek_key = build_age_seek_key(30);
        iter.apply_cursor_seek(seek_key.clone(), true, true)
            .await
            .unwrap();

        // The reversed=true path must set upper_bound_key, not lower_bound_key.
        assert!(
            iter.upper_bound_key.is_some(),
            "reversed=true must set upper_bound_key"
        );
        assert!(
            iter.lower_bound_key.is_none(),
            "reversed=true must NOT set lower_bound_key"
        );
        assert_eq!(iter.upper_bound_key.as_deref(), Some(seek_key.as_slice()));
        assert!(
            iter.upper_inclusive,
            "inclusive=true must set upper_inclusive"
        );

        // Behavioral check: docs 2 and 1 returned (inclusive at 30, exclude doc 3 > 30).
        let entries = iter.collect_all().await.unwrap();
        let doc_ids: Vec<u64> = entries.iter().map(|e| e.doc_short_id).collect();
        assert!(!doc_ids.contains(&3), "doc 3 is above the cursor boundary");
        assert!(doc_ids.contains(&2), "doc 2 is at the boundary (inclusive)");
        assert!(doc_ids.contains(&1), "doc 1 is before boundary");
        assert_eq!(entries.len(), 2);
    })
    .unwrap()
}

#[test]
fn exclusive_range_bounds_survive_reverse_and_reset() {
    workers::block_on(async {
        // 255 exercises a carry when computing the successor of an encoded bound.
        for descending in [false, true] {
            let store = RegolithStore::in_memory().unwrap();
            let mut txn = store.new_txn(false).await.unwrap();
            let mut desc = test_index_description();
            desc.fields[0].descending = descending;
            let index = SimpleIndex::new(1, desc.clone());
            for (id, age) in [(1, 254), (2, 255), (3, 255), (4, 256)] {
                index
                    .save(&mut txn, id, &[NormalValue::Int(age)])
                    .await
                    .unwrap();
            }
            txn.commit().await.unwrap();
            let txn = store.new_txn(true).await.unwrap();
            for reverse in [false, true] {
                let (lower, upper) = if descending {
                    (Bound::Unbounded, Bound::Exclusive(NormalValue::Int(255)))
                } else {
                    (Bound::Exclusive(NormalValue::Int(255)), Bound::Unbounded)
                };
                let mut iter = index
                    .scan_range(&txn, &[], lower, upper, reverse)
                    .await
                    .unwrap();
                for _ in 0..2 {
                    let entries = iter.collect_all().await.unwrap();
                    assert_eq!(
                        entries
                            .iter()
                            .map(|entry| entry.doc_short_id)
                            .collect::<Vec<_>>(),
                        vec![4]
                    );
                    iter.reset().await.unwrap();
                }
                // Moving the exclusive start beyond the inclusive end must still
                // produce an empty scan, including after reset.
                let (lower, upper) = if descending {
                    (
                        Bound::Inclusive(NormalValue::Int(255)),
                        Bound::Exclusive(NormalValue::Int(255)),
                    )
                } else {
                    (
                        Bound::Exclusive(NormalValue::Int(255)),
                        Bound::Inclusive(NormalValue::Int(255)),
                    )
                };
                let mut empty = index
                    .scan_range(&txn, &[], lower, upper, reverse)
                    .await
                    .unwrap();
                assert!(empty.collect_all().await.unwrap().is_empty());
                empty.reset().await.unwrap();
                assert!(empty.collect_all().await.unwrap().is_empty());
            }
        }
    })
    .unwrap()
}

/// Counts the entries each iterator it opens hands out.
struct CountingReader<'a, R>
where
    R: ?Sized,
{
    inner: &'a R,
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

struct CountingIterator {
    inner: Box<dyn Iterator>,
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl crate::corekv::private::Sealed for CountingIterator {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Iterator for CountingIterator {
    async fn next(&mut self) -> Result<Option<crate::corekv::KvPair>> {
        let pair = self.inner.next().await?;
        if pair.is_some() {
            self.reads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(pair)
    }

    async fn close(&mut self) -> Result<()> {
        self.inner.close().await
    }

    async fn seek(&mut self, key: &[u8]) -> Result<bool> {
        self.inner.seek(key).await
    }

    async fn reset(&mut self) -> Result<()> {
        self.inner.reset().await
    }
}

impl<R> crate::corekv::private::Sealed for CountingReader<'_, R> where R: ?Sized {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<R> Reader for CountingReader<'_, R>
where
    R: Reader + ?Sized,
{
    async fn get(&self, key: &[u8]) -> Result<Option<bytes::Bytes>> {
        self.inner.get(key).await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.inner.has(key).await
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.inner.get_size(key).await
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        Ok(Box::new(CountingIterator {
            inner: self.inner.iterator(opts).await?,
            reads: self.reads.clone(),
        }))
    }
}

#[test]
fn a_range_stopping_below_the_markers_reads_none_of_them() {
    use document::{JsonLeafValue, JsonPath, JsonScalarValue};

    let at = |value| {
        NormalValue::JsonLeaf(JsonLeafValue::new(
            JsonPath::new().append_property("tags").append_index(),
            value,
        ))
    };
    workers::block_on(async {
        for descending in [false, true] {
            let store = RegolithStore::in_memory().unwrap();
            let mut txn = store.new_txn(false).await.unwrap();
            let mut desc = test_index_description();
            desc.fields[0].descending = descending;
            let index = SimpleIndex::new(1, desc.clone());
            for id in 1..=20 {
                index
                    .save(&mut txn, id, &[at(JsonScalarValue::EmptyArray)])
                    .await
                    .unwrap();
            }
            for (id, name) in [(21, "a"), (22, "n"), (23, "z")] {
                index
                    .save(&mut txn, id, &[at(JsonScalarValue::String(name.into()))])
                    .await
                    .unwrap();
            }
            txn.commit().await.unwrap();
            let txn = store.new_txn(true).await.unwrap();
            let m = Bound::Exclusive(at(JsonScalarValue::String("m".into())));
            // The markers sit at the top of the path, past every value, so
            // the planner's `_gt` stops just below them.
            let markers = Bound::Exclusive(at(JsonScalarValue::EmptyArray));
            let (lower, upper) = if descending {
                (markers, m)
            } else {
                (m, markers)
            };
            for reverse in [false, true] {
                let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let reader = CountingReader {
                    inner: txn.as_ref(),
                    reads: reads.clone(),
                };
                let mut iter = RangeIterator::new_range(
                    &reader,
                    1,
                    &desc,
                    false,
                    &[],
                    lower.clone(),
                    upper.clone(),
                    reverse,
                )
                .await
                .unwrap();
                let mut ids: Vec<u64> = iter
                    .collect_all()
                    .await
                    .unwrap()
                    .iter()
                    .map(|entry| entry.doc_short_id)
                    .collect();
                ids.sort_unstable();
                assert_eq!(
                    ids,
                    vec![22, 23],
                    "descending {descending} reverse {reverse}"
                );
                assert!(
                    reads.load(std::sync::atomic::Ordering::Relaxed) <= 3,
                    "descending {descending} reverse {reverse}: {} reads",
                    reads.load(std::sync::atomic::Ordering::Relaxed)
                );
            }
            let mut empty = index
                .scan_prefix(&txn, &[at(JsonScalarValue::EmptyArray)], false)
                .await
                .unwrap();
            assert_eq!(empty.collect_all().await.unwrap().len(), 20);
        }
    })
    .unwrap()
}

//! Document values: part operands, their CRDT order and the parts they touch.

use crdt::field::{CounterField, LwwField, TextField};
use crdt::stored::{StoredDocument, StoredField, ValueKind};
use document::NormalValue;
use regolith::MergeOperator;

use crate::corekv::Store;
use crate::merge::{document_parts, DefraMergeOperator};
use crate::RegolithStore;

fn test_collection() -> schema::CollectionVersion {
    (1..=20)
        .fold(
            schema::CollectionBuilder::new("Test", "bafytest"),
            |builder, id| {
                builder.field(schema::FieldDescription::new(
                    id.to_string(),
                    format!("field{id}"),
                    schema::FieldKind::Scalar(schema::ScalarKind::None),
                    schema::CType::LwwRegister,
                ))
            },
        )
        .build()
}

fn field(id: u32, priority: u64, value: NormalValue) -> StoredDocument {
    StoredDocument {
        fields: [(
            id,
            LwwField::stored(format!("field{id}"), priority, &value).unwrap(),
        )]
        .into(),
    }
}

#[test]
fn priorities_ties_and_tombstones_survive_materialization() {
    let base = field(1, 1, NormalValue::Int(10));
    let replacement = field(1, 2, NormalValue::Int(20));
    let tombstone = field(1, 3, NormalValue::Null);
    let stale = field(1, 2, NormalValue::Int(99));
    let operands: Vec<_> = [&replacement, &tombstone, &stale]
        .into_iter()
        .map(|value| document_parts(value).unwrap())
        .collect();
    let bytes = DefraMergeOperator
        .full_merge(
            b"doc",
            Some(&base.encode().unwrap()),
            &operands.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        )
        .unwrap();
    let stored = StoredDocument::decode(&bytes).unwrap();
    assert_eq!(stored.fields[&1].priority, 3);
    assert!(stored.fields[&1].value.is_empty());
    assert!(stored
        .materialize(test_collection())
        .unwrap()
        .get("field1")
        .is_none());
    let a = document_parts(&field(1, 4, NormalValue::String("a".into()))).unwrap();
    let b = document_parts(&field(1, 4, NormalValue::String("b".into()))).unwrap();
    assert_eq!(
        DefraMergeOperator.full_merge(b"doc", Some(&bytes), &[&a, &b]),
        DefraMergeOperator.full_merge(b"doc", Some(&bytes), &[&b, &a])
    );
}

#[test]
fn document_folds_and_touches_obey_engine_contracts() {
    use regolith::testing::proptest::prelude::*;
    let values = (0u32..4, any::<u64>(), any::<i64>())
        .prop_map(|(id, priority, value)| field(id, priority, NormalValue::Int(value)));
    let operands = values
        .clone()
        .prop_map(|value| document_parts(&value).unwrap());
    let checked = regolith::testing::check_merge_operator(
        &DefraMergeOperator,
        Just(b"doc".to_vec()),
        values.prop_map(|value| value.encode().unwrap()),
        operands.clone(),
    )
    .unwrap();
    assert!(checked.folds > 0);
    let checked = regolith::testing::check_touches(
        &DefraMergeOperator,
        Just(b"doc".to_vec()),
        operands,
        regolith::testing::proptest::collection::vec(0u32..5, 0..6),
    )
    .unwrap();
    assert!(checked.folds > 0);
}

/// Parts 0 to 2 are counters of `kind`, part 3 an LWW field; each part keeps
/// one name, as a document value's parts do.
fn mixed_parts(
    kind: ValueKind,
) -> impl regolith::testing::proptest::strategy::Strategy<Value = StoredDocument> + Clone {
    use regolith::testing::proptest::prelude::*;
    let part = (0u32..4, 1u64..4, -1000i64..1000).prop_map(move |(id, priority, value)| {
        let field = if id < 3 {
            CounterField::stored(
                format!("count{id}"),
                kind,
                priority,
                &NormalValue::Int(value),
            )
            .unwrap()
        } else {
            LwwField::stored(format!("field{id}"), priority, &NormalValue::Int(value)).unwrap()
        };
        (id, field)
    });
    regolith::testing::proptest::collection::vec(part, 1..3).prop_map(|parts| StoredDocument {
        fields: parts.into_iter().collect(),
    })
}

/// Integer counter parts fold exactly in any grouping, alone or beside LWW
/// parts, and a folded operand touches exactly the parts its operands touch.
#[test]
fn integer_counter_parts_fold_exactly_and_touch_only_their_parts() {
    let documents = mixed_parts(ValueKind::Int64);
    let operands = {
        use regolith::testing::proptest::prelude::*;
        documents
            .clone()
            .prop_map(|value| document_parts(&value).unwrap())
    };
    let checked = regolith::testing::check_merge_operator(
        &DefraMergeOperator,
        regolith::testing::proptest::prelude::Just(b"doc".to_vec()),
        {
            use regolith::testing::proptest::prelude::*;
            documents.prop_map(|value| value.encode().unwrap())
        },
        operands.clone(),
    )
    .unwrap();
    assert!(checked.folds > 0, "integer counter operands must fold");
    let count = |value: i64| {
        document_parts(&StoredDocument {
            fields: [(
                0,
                CounterField::stored(
                    "count0".into(),
                    ValueKind::Int64,
                    1,
                    &NormalValue::Int(value),
                )
                .unwrap(),
            )]
            .into(),
        })
        .unwrap()
    };
    assert_eq!(
        DefraMergeOperator.partial_merge(b"doc", &count(i64::MAX), &count(2)),
        Some(count(i64::MIN + 1)),
        "two increments of one counter fold into their wrapping sum"
    );
    let checked = regolith::testing::check_touches(
        &DefraMergeOperator,
        regolith::testing::proptest::prelude::Just(b"doc".to_vec()),
        operands,
        regolith::testing::proptest::collection::vec(0u32..5, 0..6),
    )
    .unwrap();
    assert!(checked.folds > 0);
}

/// Floating-point counter parts keep their order: no early fold, and
/// `full_merge` agrees with applying the operands one at a time.
#[test]
fn float_counter_parts_are_not_folded_early() {
    use regolith::testing::proptest::prelude::*;
    for kind in [ValueKind::Float32, ValueKind::Float64] {
        let documents = mixed_parts(kind);
        regolith::testing::check_merge_operator(
            &DefraMergeOperator,
            Just(b"doc".to_vec()),
            documents.clone().prop_map(|value| value.encode().unwrap()),
            documents.prop_map(|value| document_parts(&value).unwrap()),
        )
        .unwrap();
    }
    let float = |value: f64| {
        document_parts(&StoredDocument {
            fields: [(
                0,
                CounterField::stored(
                    "count".into(),
                    ValueKind::Float64,
                    1,
                    &NormalValue::Float64(value),
                )
                .unwrap(),
            )]
            .into(),
        })
        .unwrap()
    };
    assert!(DefraMergeOperator
        .partial_merge(b"doc", &float(1e20), &float(-1e20))
        .is_none());
}

/// A counter part the stored value lacks starts from zero: a negative-zero
/// increment reads back as zero, as it would on a stored zero.
#[test]
fn an_absent_counter_part_starts_from_zero() {
    let increment = document_parts(&StoredDocument {
        fields: [(
            0,
            CounterField::stored(
                "count".into(),
                ValueKind::Float64,
                1,
                &NormalValue::Float64(-0.0),
            )
            .unwrap(),
        )]
        .into(),
    })
    .unwrap();
    let bytes = DefraMergeOperator
        .full_merge(b"doc", None, &[&increment])
        .unwrap();
    let stored = StoredDocument::decode(&bytes).unwrap();
    assert_eq!(stored.fields[&0].value, 0.0f64.to_be_bytes());
}

#[test]
fn disjoint_document_operands_commit_and_projected_reads_validate_only_named_fields() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut seed = store.new_txn(false).await.unwrap();
        seed.set(b"doc", &field(1, 1, NormalValue::Int(10)).encode().unwrap())
            .await
            .unwrap();
        seed.commit().await.unwrap();
        let mut first = store.new_txn(false).await.unwrap();
        let mut second = store.new_txn(false).await.unwrap();
        first.get_parts(b"doc", &[1]).await.unwrap();
        first
            .merge(
                b"doc",
                &document_parts(&field(1, 2, NormalValue::Int(11))).unwrap(),
            )
            .await
            .unwrap();
        second
            .merge(
                b"doc",
                &document_parts(&field(2, 1, NormalValue::Int(22))).unwrap(),
            )
            .await
            .unwrap();
        second.commit().await.unwrap();
        first.commit().await.unwrap();
        let mut stale = store.new_txn(false).await.unwrap();
        stale.get_parts(b"doc", &[1]).await.unwrap();
        let mut update = store.new_txn(false).await.unwrap();
        update
            .merge(
                b"doc",
                &document_parts(&field(1, 3, NormalValue::Int(12))).unwrap(),
            )
            .await
            .unwrap();
        update.commit().await.unwrap();
        stale.set(b"decision", b"old").await.unwrap();
        assert!(stale.commit().await.is_err());
        let reader = store.new_txn(true).await.unwrap();
        let stored = StoredDocument::decode(&reader.get(b"doc").await.unwrap().unwrap()).unwrap();
        let doc = stored.materialize(test_collection()).unwrap();
        assert_eq!(doc.get("field1"), Some(&NormalValue::Int(12)));
        assert_eq!(doc.get("field2"), Some(&NormalValue::Int(22)));
    })
    .unwrap()
}

#[test]
fn numeric_document_parts_add_without_adopting_a_stale_mirror() {
    let counter = |value: i64| StoredDocument {
        fields: [(
            7,
            StoredField {
                name: "count".into(),
                kind: ValueKind::Int64,
                priority: 1,
                value: value.to_be_bytes().to_vec(),
            },
        )]
        .into(),
    };
    let bytes = DefraMergeOperator
        .full_merge(
            b"doc",
            Some(&counter(10).encode().unwrap()),
            &[
                &document_parts(&counter(3)).unwrap(),
                &document_parts(&counter(5)).unwrap(),
            ],
        )
        .unwrap();
    assert_eq!(
        StoredDocument::decode(&bytes)
            .unwrap()
            .materialize(test_collection())
            .unwrap()
            .get("count"),
        Some(&NormalValue::Int(18))
    );
}

#[test]
fn corrupt_encodings_are_refused_without_trusting_lengths() {
    let bytes = field(1, 1, NormalValue::Int(1)).encode().unwrap();
    for end in 0..bytes.len() {
        assert!(StoredDocument::decode(&bytes[..end]).is_none());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(StoredDocument::decode(&trailing).is_none());
    let mut huge = bytes;
    huge[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(StoredDocument::decode(&huge).is_none());
    assert!(DefraMergeOperator.touches(b"doc", b"DM\x01\x05broken", &[1]));
}

#[test]
fn encrypted_document_operands_survive_reopen_and_schema_put_terminates_old_operands() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let options = crate::RegolithStoreOptions::default().with_encryption_key([23; 32]);
        {
            let store = RegolithStore::open_with_options(dir.path(), options.clone()).unwrap();
            let mut seed = store.new_txn(false).await.unwrap();
            seed.set(b"doc", &field(1, 1, NormalValue::Int(10)).encode().unwrap())
                .await
                .unwrap();
            seed.commit().await.unwrap();
            let mut update = store.new_txn(false).await.unwrap();
            update
                .merge(
                    b"doc",
                    &document_parts(&field(1, 2, NormalValue::Int(20))).unwrap(),
                )
                .await
                .unwrap();
            update.commit().await.unwrap();
            store.close().await.unwrap();
        }
        let store = RegolithStore::open_with_options(dir.path(), options).unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        let bytes = txn.get(b"doc").await.unwrap().unwrap();
        let doc = StoredDocument::decode(&bytes)
            .unwrap()
            .materialize(test_collection())
            .unwrap();
        assert_eq!(
            doc.to_cbor().unwrap(),
            document::Document::from_json_str(r#"{"field1":20}"#, test_collection())
                .unwrap()
                .to_cbor()
                .unwrap()
        );
        txn.merge(
            b"doc",
            &document_parts(&field(1, 3, NormalValue::Int(30))).unwrap(),
        )
        .await
        .unwrap();
        // A version transition replaces both values and ordering metadata.
        txn.set(b"doc", &field(2, 1, NormalValue::Int(40)).encode().unwrap())
            .await
            .unwrap();
        txn.commit().await.unwrap();
        let reader = store.new_txn(true).await.unwrap();
        let doc = StoredDocument::decode(&reader.get(b"doc").await.unwrap().unwrap())
            .unwrap()
            .materialize(test_collection())
            .unwrap();
        assert_eq!(doc.get("field1"), None);
        assert_eq!(doc.get("field2"), Some(&NormalValue::Int(40)));
    })
    .unwrap()
}

/// Floating-point counter parts beside LWW parts: a fold, where one happens,
/// touches exactly the parts its operands touch.
#[test]
fn float_counter_parts_keep_touches_through_folds() {
    use regolith::testing::proptest::prelude::*;
    for kind in [ValueKind::Float32, ValueKind::Float64] {
        regolith::testing::check_touches(
            &DefraMergeOperator,
            Just(b"doc".to_vec()),
            mixed_parts(kind).prop_map(|value| document_parts(&value).unwrap()),
            regolith::testing::proptest::collection::vec(0u32..5, 0..6),
        )
        .unwrap();
    }
}

/// The half of `touches`'s contract `check_touches` leaves to the caller: a
/// part no operand touches keeps its bytes through `full_merge`, and the
/// operator says the operands leave it alone.
#[test]
fn a_part_no_operand_touches_keeps_its_bytes() {
    use regolith::testing::proptest::collection::vec;
    use regolith::testing::proptest::test_runner::{Config, TestCaseError, TestRunner};

    for kind in [ValueKind::Int64, ValueKind::Float64] {
        let mut runner = TestRunner::new(Config {
            failure_persistence: None,
            ..Config::default()
        });
        runner
            .run(
                &(mixed_parts(kind), vec(mixed_parts(kind), 1..6)),
                |(base, operands)| {
                    let encoded: Vec<Vec<u8>> = operands
                        .iter()
                        .map(|operand| document_parts(operand).unwrap())
                        .collect();
                    let refs: Vec<&[u8]> = encoded.iter().map(Vec::as_slice).collect();
                    let merged = DefraMergeOperator
                        .full_merge(b"doc", Some(&base.encode().unwrap()), &refs)
                        .ok_or_else(|| TestCaseError::fail("the operands must merge"))?;
                    let merged = StoredDocument::decode(&merged)
                        .ok_or_else(|| TestCaseError::fail("the merged value must decode"))?;
                    for (id, field) in &base.fields {
                        if operands
                            .iter()
                            .any(|operand| operand.fields.contains_key(id))
                        {
                            continue;
                        }
                        if encoded
                            .iter()
                            .any(|operand| DefraMergeOperator.touches(b"doc", operand, &[*id]))
                        {
                            return Err(TestCaseError::fail(format!(
                                "an operand without part {id} touches it"
                            )));
                        }
                        if merged.fields.get(id) != Some(field) {
                            return Err(TestCaseError::fail(format!(
                                "part {id} changed though no operand touches it"
                            )));
                        }
                    }
                    Ok(())
                },
            )
            .unwrap();
    }
}

/// A one-part operand written from a borrowed part is the operand of the
/// stored document holding that one field.
#[test]
fn a_borrowed_part_writes_the_operand_of_its_document() {
    let counter = CounterField::stored(
        "count".to_owned(),
        ValueKind::Int64,
        4,
        &NormalValue::Int(7),
    )
    .unwrap();
    let part = crdt::stored::Part {
        name: &counter.name,
        kind: counter.kind,
        priority: counter.priority,
        value: &counter.value,
    };
    let whole = StoredDocument {
        fields: [(3, counter.clone())].into(),
    };
    assert_eq!(crate::merge::document_part(3, part), document_parts(&whole));
}

/// Parts 0 and 1 are Text fields, part 2 an LWW field. A Text operand
/// replaces in commit order, so folds keep the later text in any grouping and
/// a folded operand touches exactly the parts its operands touch.
#[test]
fn text_parts_replace_in_commit_order_and_obey_engine_contracts() {
    use regolith::testing::proptest::prelude::*;
    let part = (0u32..3, 1u64..4, "[a-c]{0,3}").prop_map(|(id, priority, text)| {
        let value = NormalValue::String(text);
        let field = if id < 2 {
            TextField::stored(format!("text{id}"), priority, &value).unwrap()
        } else {
            LwwField::stored(format!("field{id}"), priority, &value).unwrap()
        };
        (id, field)
    });
    let documents =
        regolith::testing::proptest::collection::vec(part, 1..3).prop_map(|parts| StoredDocument {
            fields: parts.into_iter().collect(),
        });
    let operands = documents
        .clone()
        .prop_map(|value| document_parts(&value).unwrap());
    let checked = regolith::testing::check_merge_operator(
        &DefraMergeOperator,
        Just(b"doc".to_vec()),
        documents.prop_map(|value| value.encode().unwrap()),
        operands.clone(),
    )
    .unwrap();
    assert!(checked.folds > 0, "text operands must fold");
    let checked = regolith::testing::check_touches(
        &DefraMergeOperator,
        Just(b"doc".to_vec()),
        operands,
        regolith::testing::proptest::collection::vec(0u32..4, 0..5),
    )
    .unwrap();
    assert!(checked.folds > 0);

    let text = |priority, value: &str| {
        document_parts(&StoredDocument {
            fields: [(
                0,
                TextField::stored("text0".into(), priority, &NormalValue::String(value.into()))
                    .unwrap(),
            )]
            .into(),
        })
        .unwrap()
    };
    let merged = DefraMergeOperator
        .full_merge(b"doc", None, &[&text(5, "zzz"), &text(2, "abc")])
        .unwrap();
    let stored = StoredDocument::decode(&merged).unwrap();
    assert_eq!(
        stored.fields[&0].materialize(),
        Some(NormalValue::String("abc".into()))
    );
}

/// A Json part on field 7: entries and removes drawn from a few paths and
/// blocks, so operands overlap, remove each other's entries and re-add them.
fn json_part(
) -> impl regolith::testing::proptest::strategy::Strategy<Value = StoredDocument> + Clone {
    use defra_core::json_crdt::{Entry, Frontier, JsonEdit};
    use regolith::testing::proptest::prelude::*;
    let paths = || {
        prop_oneof![
            Just(vec![]),
            Just(vec!["a".to_owned()]),
            Just(vec!["a".to_owned(), "b".to_owned()]),
            Just(vec!["c".to_owned()]),
        ]
    };
    let entry = (paths(), 0u8..4, 0u8..3).prop_map(|(path, block, edit)| Entry {
        path,
        block: vec![block],
        priority: u64::from(block),
        edit: match edit {
            0 => JsonEdit::Set(serde_json::json!(block)),
            1 => JsonEdit::Map,
            _ => JsonEdit::Delete,
        },
        seen: vec![vec![block.saturating_sub(1)]],
    });
    let remove = (paths(), 0u8..4).prop_map(|(path, block)| (path, vec![block]));
    (
        regolith::testing::proptest::collection::vec(remove, 0..3),
        regolith::testing::proptest::collection::vec(entry, 0..3),
    )
        .prop_map(|(removes, entries)| StoredDocument {
            fields: [(
                7,
                StoredField {
                    name: "meta".to_owned(),
                    kind: ValueKind::Json,
                    priority: 1,
                    value: Frontier { removes, entries }.encode().unwrap(),
                },
            )]
            .into(),
        })
}

#[test]
fn json_parts_compose_exactly_and_touch_only_their_field() {
    use regolith::testing::proptest::strategy::{Just, Strategy};
    let checked = regolith::testing::check_merge_operator(
        &DefraMergeOperator,
        Just(b"doc".to_vec()),
        json_part().prop_map(|value| {
            let mut settled = StoredDocument::default();
            settled.apply(value).unwrap();
            settled.encode().unwrap()
        }),
        json_part().prop_map(|value| document_parts(&value).unwrap()),
    )
    .unwrap();
    assert!(checked.folds > 0);
    let checked = regolith::testing::check_touches(
        &DefraMergeOperator,
        Just(b"doc".to_vec()),
        json_part().prop_map(|value| document_parts(&value).unwrap()),
        regolith::testing::proptest::collection::vec(5u32..9, 0..3),
    )
    .unwrap();
    assert!(checked.folds > 0);
}

/// `Collection::put_document_value` rewrites a whole document value on a
/// base it read with `get_parts(key, &[])`, which validates nothing. A blind
/// Json operand that commits in between is still not lost: the put writes the
/// key, a newer version of a written key conflicts at commit, and the retry
/// reads the merged value.
#[test]
fn a_whole_document_put_conflicts_with_a_json_operand_committed_after_its_read() {
    use defra_core::json_crdt::{Entry, Frontier, JsonEdit};
    workers::block_on(async {
        let json = |block: u8| StoredDocument {
            fields: [(
                7,
                StoredField {
                    name: "meta".to_owned(),
                    kind: ValueKind::Json,
                    priority: u64::from(block),
                    value: Frontier {
                        removes: Vec::new(),
                        entries: vec![Entry {
                            path: vec![format!("k{block}")],
                            block: vec![block],
                            priority: u64::from(block),
                            edit: JsonEdit::Set(serde_json::json!(block)),
                            seen: Vec::new(),
                        }],
                    }
                    .encode()
                    .unwrap(),
                },
            )]
            .into(),
        };
        let store = RegolithStore::in_memory().unwrap();
        let mut seed = store.new_txn(false).await.unwrap();
        seed.set(b"doc", &json(1).encode().unwrap()).await.unwrap();
        seed.commit().await.unwrap();

        let mut put = store.new_txn(false).await.unwrap();
        let base = put.get_parts(b"doc", &[]).await.unwrap().unwrap();
        let mut merge = store.new_txn(false).await.unwrap();
        merge
            .merge(b"doc", &document_parts(&json(2)).unwrap())
            .await
            .unwrap();
        merge.commit().await.unwrap();
        let mut whole = StoredDocument::decode(&base).unwrap();
        whole.fields.extend(field(1, 2, NormalValue::Int(5)).fields);
        put.set(b"doc", &whole.encode().unwrap()).await.unwrap();
        assert!(put.commit().await.is_err(), "the stale put must conflict");

        let reader = store.new_txn(true).await.unwrap();
        let stored = StoredDocument::decode(&reader.get(b"doc").await.unwrap().unwrap()).unwrap();
        let frontier = Frontier::decode(&stored.fields[&7].value).unwrap();
        assert_eq!(frontier.entries.len(), 2, "the operand survives");
        assert!(!stored.fields.contains_key(&1));
    })
    .unwrap()
}

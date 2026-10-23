use super::*;
use defra_core::block_delta::CounterDeltaPayload;
use document::NormalValue;
use schema::FieldDescription;

fn counters(kind: FieldKind) -> Arc<CollectionVersion> {
    Arc::new(CollectionVersion::new(
        "Counters",
        "v1",
        "col-counters",
        vec![FieldDescription::new("1", "score", kind, CType::PnCounter)],
    ))
}

fn increments<T>(values: &[T]) -> Vec<(Cid, Block)>
where
    T: serde::Serialize,
{
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let mut data = Vec::new();
            ciborium::into_writer(value, &mut data).unwrap();
            let block = Block::new(
                CrdtDelta::Counter(CounterDeltaPayload {
                    field_name: "score".into(),
                    priority: index as u64 + 1,
                    nonce: index as i64,
                    schema_version_id: "v1".into(),
                    data,
                }),
                Vec::new(),
                Vec::new(),
            );
            (block.generate_cid().unwrap(), block)
        })
        .collect()
}

fn replayed(collection: &Arc<CollectionVersion>, blocks: &[(Cid, Block)]) -> NormalValue {
    let doc_id = "bae-0000";
    replay_deltas(&crdt::Crdts::default(), collection, blocks, doc_id)
        .unwrap()
        .get("score")
        .cloned()
        .unwrap()
}

/// Go adds counter increments with `+`, which wraps on overflow; a replay
/// once saturated, so a historical read disagreed with the live value.
#[test]
fn an_integer_counter_replay_wraps_as_go_does() {
    let blocks = increments(&[i64::MAX, 1]);
    assert_eq!(
        replayed(&counters(FieldKind::int()), &blocks),
        NormalValue::Int(i64::MIN)
    );
}

/// A Float32 counter adds at `f32` precision and reads back as the stored
/// value reads, the same in a replay as in a live merge.
#[test]
fn a_float32_counter_replay_reads_as_the_stored_value() {
    let blocks = increments(&[0.1f32, 0.1, 0.1]);
    let sum = 0.1f32 + 0.1f32 + 0.1f32;
    assert_eq!(
        replayed(&counters(FieldKind::float32()), &blocks),
        NormalValue::Float64(f64::from(sum))
    );
}

/// A counter block on a field declared LWW is skipped, as a live merge refuses
/// it: the field keeps its LWW value, or stays absent without one.
#[test]
fn a_counter_block_on_an_lww_field_is_skipped_in_a_replay() {
    let collection = Arc::new(CollectionVersion::new(
        "Counters",
        "v1",
        "col-counters",
        vec![FieldDescription::new(
            "1",
            "score",
            FieldKind::int(),
            CType::LwwRegister,
        )],
    ));
    let mut data = Vec::new();
    ciborium::into_writer(&7_i64, &mut data).unwrap();
    let set = Block::new(
        CrdtDelta::Lww(defra_core::block_delta::LwwDeltaPayload {
            field_name: "score".into(),
            priority: 1,
            schema_version_id: "v1".into(),
            data,
        }),
        Vec::new(),
        Vec::new(),
    );
    let set = (set.generate_cid().unwrap(), set);
    let increment = increments(&[5_i64]).remove(0);

    let alone = replay_deltas(
        &crdt::Crdts::default(),
        &collection,
        std::slice::from_ref(&increment),
        "bae-0000",
    )
    .unwrap();
    assert_eq!(alone.get("score"), None);
    assert_eq!(
        replayed(&collection, &[set, increment]),
        NormalValue::Int(7)
    );
}

/// A Text block that does not decode fails the read with its decode error;
/// skipping it would leave its child without a parent version.
#[test]
fn an_undecodable_text_block_fails_the_replay_with_its_error() {
    let collection = Arc::new(CollectionVersion::new(
        "Notes",
        "v1",
        "col-notes",
        vec![FieldDescription::new(
            "1",
            "body",
            FieldKind::string(),
            CType::Text,
        )],
    ));
    let text = |priority, data, heads| {
        let block = Block::new(
            CrdtDelta::Text(defra_core::block_delta::TextDeltaPayload {
                field_name: "body".into(),
                priority,
                schema_version_id: "v1".into(),
                data,
            }),
            heads,
            Vec::new(),
        );
        (block.generate_cid().unwrap(), block)
    };
    let broken = text(1, vec![0xff], Vec::new());
    let edits = defra_core::TextEdits {
        ops: defra_core::TextOp::diff("", "a"),
        len: 1,
    };
    let child = text(2, edits.encode().unwrap(), vec![broken.0]);

    let error = replay_deltas(
        &crdt::Crdts::default(),
        &collection,
        &[broken.clone(), child],
        "bae-0000",
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains(&broken.0.to_string()), "{error}");
    assert!(error.contains("text ops"), "{error}");
}

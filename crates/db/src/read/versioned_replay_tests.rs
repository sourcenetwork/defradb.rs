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
                    collection_version_id: "v1".into(),
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
    replay_deltas(collection, blocks, doc_id)
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

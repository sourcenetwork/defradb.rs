//! The database's receive gate (#2456): the filter and the retention floor
//! judged on whatever blocks of a pushed document are at hand, before any
//! is fetched or stored.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use bytes::Bytes;
use db::database::{DbOptions, DB};
use db::receive_gate::DbReceiveGate;
use db::replication_filter::{FieldValues, ReplicationFilter};
use db::retention::{RetentionFloor, StaticRetentionRule};
use defra_core::block::{Block, CrdtDelta};
use document::{Document, NormalValue};
use p2p::sync::receive_gate::{CarBlocks, GateRefusal, ReceiveGate};
use rapidhash::{HashMapExt, RapidHashMap};
use storage::RegolithStore;

use super::retention::logs;

/// Refuses a document whose `blockNumber` is at or below the cutoff.
struct Cutoff(i64);

impl ReplicationFilter for Cutoff {
    fn allow_replication(&self, _collection: &str, _doc: &str, fields: &FieldValues) -> bool {
        match fields.get("blockNumber") {
            Some(ciborium::Value::Integer(value)) => i128::from(*value) > i128::from(self.0),
            _ => true,
        }
    }
}

async fn gate(
    options: DbOptions,
) -> (
    DbReceiveGate<RegolithStore>,
    Arc<DefraBlockstore<RegolithStore>>,
) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc_with_options(store.clone(), options).unwrap());
    db.create_collection(logs("col-log")).await.unwrap();
    let blocks = Arc::new(DefraBlockstore::new(store, false));
    (DbReceiveGate::new(db).expect("a gate"), blocks)
}

fn floor(at: i64) -> DbOptions {
    let mut floors = RapidHashMap::new();
    floors.insert(
        "col-log".to_string(),
        RetentionFloor {
            field: "blockNumber".to_string(),
            floor: at,
        },
    );
    DbOptions::new().with_retention_rule(Arc::new(StaticRetentionRule::new(floors)))
}

/// A Log's head and, when `with_fields`, the blocks it links.
async fn pushed(
    blocks: &Arc<DefraBlockstore<RegolithStore>>,
    height: Option<i64>,
    with_fields: bool,
) -> (String, Vec<u8>, Vec<(cid::Cid, Bytes)>) {
    let mut doc = Document::new(logs("col-log"));
    doc.set("address", NormalValue::String("0xabc".into()))
        .unwrap();
    if let Some(height) = height {
        doc.set("blockNumber", NormalValue::Int(height)).unwrap();
    }
    let built = db::block::builder::build_blocks_from_document(
        &storage::Crdts::default(),
        &doc,
        &logs("col-log"),
        blocks,
    )
    .await
    .unwrap();
    let mut linked = Vec::new();
    if with_fields {
        let head = Block::from_dag_cbor(&built.block).unwrap();
        for link in head.links.into_iter().flatten() {
            let data = blocks.get(&link.link).await.unwrap().unwrap();
            linked.push((link.link, data));
        }
    }
    (built.doc_id, built.block.to_vec(), linked)
}

async fn judge(
    gate: &DbReceiveGate<RegolithStore>,
    doc_id: &str,
    head: &[u8],
    linked: Vec<(cid::Cid, Bytes)>,
) -> Option<GateRefusal> {
    gate.judge("col-log", doc_id, head, &CarBlocks::new(linked, None))
        .await
}

#[test]
#[mantra_macros::req_test("go.shinzo.p2p.retention.gate")]
fn the_floor_refuses_a_create_whose_height_is_at_hand() {
    workers::block_on(async {
        let (gate, blocks) = gate(floor(100)).await;
        for (height, expected) in [
            (Some(100), Some(GateRefusal::BelowFloor)),
            (Some(101), None),
            // No height link: Go's gate lets it go on, and the merge judges it.
            (None, None),
            (Some(-5), Some(GateRefusal::NoHeight)),
        ] {
            let (doc, head, linked) = pushed(&blocks, height, true).await;
            assert_eq!(
                judge(&gate, &doc, &head, linked).await,
                expected,
                "{height:?}"
            );
        }
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.p2p.retention.lets_through")]
fn what_the_gate_cannot_read_or_judge_goes_on() {
    workers::block_on(async {
        let (gate, blocks) = gate(floor(100)).await;
        let (doc, head, _) = pushed(&blocks, Some(100), false).await;
        assert_eq!(
            judge(&gate, &doc, &head, Vec::new()).await,
            None,
            "the height block is not at hand"
        );

        let (doc, head, linked) = pushed(&blocks, Some(100), true).await;
        let mut update = Block::from_dag_cbor(&head).unwrap();
        if let CrdtDelta::Composite(payload) = &mut update.delta {
            payload.priority = 2;
        }
        let update = update.to_dag_cbor().unwrap();
        assert_eq!(
            judge(&gate, &doc, &update, linked).await,
            None,
            "an update is not judged by height"
        );
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.p2p.replication_filter.after_fetch")]
fn the_filter_sees_the_values_at_hand_and_keeps_a_head_alone() {
    workers::block_on(async {
        let options = DbOptions::new().with_replication_filter(Arc::new(Cutoff(100)));
        let (gate, blocks) = gate(options).await;
        let (doc, head, linked) = pushed(&blocks, Some(100), true).await;
        assert_eq!(
            judge(&gate, &doc, &head, Vec::new()).await,
            None,
            "a composite head alone carries no values: the filter keeps it"
        );
        assert_eq!(
            judge(&gate, &doc, &head, linked).await,
            Some(GateRefusal::Filtered)
        );
        let (doc, head, linked) = pushed(&blocks, Some(101), true).await;
        assert_eq!(judge(&gate, &doc, &head, linked).await, None);
    })
    .unwrap()
}

#[test]
fn a_node_with_nothing_to_judge_has_no_gate() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store).unwrap());
        assert!(DbReceiveGate::new(db).is_none());
    })
    .unwrap()
}

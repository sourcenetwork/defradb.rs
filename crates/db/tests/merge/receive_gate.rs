//! The database's receive gate: the replication filter asked on whatever
//! blocks of a pushed document are at hand, before any is fetched or stored.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use bytes::Bytes;
use db::database::{DbOptions, DB};
use db::receive_gate::DbReceiveGate;
use db::replication_filter::{FieldValues, ReplicationFilter};
use defra_core::block::Block;
use document::{Document, NormalValue};
use p2p::sync::receive_gate::{CarBlocks, ReceiveGate};
use storage::RegolithStore;

use super::replicated::logs;

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

/// A Log at `height`: its DocID, its head and the blocks the head links.
async fn pushed(
    blocks: &Arc<DefraBlockstore<RegolithStore>>,
    height: i64,
) -> (String, Vec<u8>, Vec<(cid::Cid, Bytes)>) {
    let mut doc = Document::new(logs("col-log"));
    doc.set("address", NormalValue::String("0xabc".into()))
        .unwrap();
    doc.set("blockNumber", NormalValue::Int(height)).unwrap();
    let built = db::block::builder::build_blocks_from_document(
        &storage::Crdts::default(),
        &doc,
        &logs("col-log"),
        blocks,
    )
    .await
    .unwrap();
    let mut linked = Vec::new();
    let head = Block::from_dag_cbor(&built.block).unwrap();
    for link in head.links.into_iter().flatten() {
        let data = blocks.get(&link.link).await.unwrap().unwrap();
        linked.push((link.link, data));
    }
    (built.doc_id, built.block.to_vec(), linked)
}

async fn allows(
    gate: &DbReceiveGate<RegolithStore>,
    doc_id: &str,
    head: &[u8],
    linked: Vec<(cid::Cid, Bytes)>,
) -> bool {
    gate.allows("col-log", doc_id, head, &CarBlocks::new(linked, None))
        .await
}

#[test]
#[mantra_macros::req_test("defra.replication_filter.receive_gate")]
fn the_filter_sees_the_values_at_hand_and_keeps_a_head_alone() {
    workers::block_on(async {
        let options = DbOptions::new().with_replication_filter(Arc::new(Cutoff(100)));
        let (gate, blocks) = gate(options).await;
        let (doc, head, linked) = pushed(&blocks, 100).await;
        assert!(
            allows(&gate, &doc, &head, Vec::new()).await,
            "a composite head alone carries no values: the filter keeps it"
        );
        assert!(!allows(&gate, &doc, &head, linked).await);
        let (doc, head, linked) = pushed(&blocks, 101).await;
        assert!(allows(&gate, &doc, &head, linked).await);
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.replication_filter.receive_gate")]
fn a_node_without_a_filter_has_no_gate() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store).unwrap());
        assert!(DbReceiveGate::new(db).is_none());
    })
    .unwrap()
}

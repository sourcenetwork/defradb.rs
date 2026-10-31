//! Merging replicated creates: one event that cannot merge does not stop the
//! others in its batch (`go.shinzo.merge.batch_isolation`), and a merged
//! document owns its blocks under its own DocID (`go.shinzo.merge.local_ids`).

use std::sync::Arc;

use blockstore::DefraBlockstore;
use db::database::{DbOptions, DB};
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{generate_cid_from_bytes, Block, CompositeDeltaPayload, CrdtDelta};
use defra_core::merge::{MergeBlock, MergeHandler, MergeOutcome};
use storage::RegolithStore;

use super::retention::{log, logs, merge, stored, Handler};

async fn node() -> (Handler, Arc<DefraBlockstore<RegolithStore>>) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc_with_options(store.clone(), DbOptions::new()).unwrap());
    db.create_collection(logs("col-log")).await.unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    (DbMergeHandler::new(db, blockstore.clone()), blockstore)
}

/// An update of a document this node never received: its head's parent is
/// a block no peer sent.
fn orphan(like: &MergeBlock) -> MergeBlock {
    let missing = generate_cid_from_bytes(b"a parent this node never received").unwrap();
    let update = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "v1".to_string(),
            priority: 2,
            status: 1,
        }),
        vec![missing],
        vec![],
    );
    MergeBlock {
        cid: update.generate_cid().unwrap(),
        block_data: bytes::Bytes::from(update.to_dag_cbor().unwrap()),
        doc_id: "bae-00000000-0000-5000-8000-000000000000".to_string(),
        ..like.clone()
    }
}

/// A batch of `size` creates with the orphan at `bad`: every other event
/// merges and is stored, the orphan neither.
async fn batch_with_an_orphan_at(size: usize, bad: usize) {
    let schema = logs("col-log");
    let (handler, blockstore) = node().await;
    let mut blocks = Vec::with_capacity(size);
    for i in 0..size {
        blocks.push(
            log(
                &blockstore,
                &schema,
                &format!("addr-{size}-{i}"),
                Some(i as i64),
            )
            .await,
        );
    }
    blocks[bad] = orphan(&blocks[bad]);

    // Through the handler the network calls, which reports a drop as Go does.
    let network = db::merge::AcpMergeHandler::new(Arc::new(DbMergeHandler::new(
        handler.db().clone(),
        blockstore.clone(),
    )));
    let outcomes = network.handle_block_batch(&blocks).await;
    let missing = generate_cid_from_bytes(b"a parent this node never received").unwrap();
    let error = outcomes[bad].as_ref().unwrap_err().to_string();
    assert!(error.starts_with("merge event dropped: "), "{error}");
    assert!(
        error.contains(&format!("could not find {missing}")),
        "{error}"
    );
    assert!(
        error.ends_with(&format!(
            ". DocID: {}, CID: {}",
            blocks[bad].doc_id, blocks[bad].cid
        )),
        "{error}"
    );
    for (i, (block, outcome)) in blocks.iter().zip(&outcomes).enumerate() {
        let merged = matches!(outcome, Ok(MergeOutcome::Merged));
        assert_eq!(merged, i != bad, "event {i} of {size}: {outcome:?}");
        assert_eq!(
            stored(&handler, "col-log", &block.doc_id).await,
            i != bad,
            "event {i} of {size}"
        );
    }
}

#[test]
#[mantra_macros::req_test("go.shinzo.merge.batch_isolation")]
fn an_event_whose_parent_never_arrived_stops_no_other_event() {
    workers::block_on(async {
        batch_with_an_orphan_at(2, 0).await;
        batch_with_an_orphan_at(18, 8).await;
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.merge.local_ids")]
fn a_merged_document_owns_its_field_blocks_under_its_doc_id() {
    workers::block_on(async {
        let schema = logs("col-log");
        let (handler, blockstore) = node().await;
        let created = log(&blockstore, &schema, "addr", Some(7)).await;
        assert_eq!(merge(&handler, &created).await, MergeOutcome::Merged);
        let composite = Block::from_dag_cbor(&created.block_data).unwrap();
        let txn = handler.db().new_txn(true).await.unwrap();
        let systemstore = txn.systemstore().unwrap();
        let fields: Vec<_> = composite
            .links
            .unwrap()
            .into_iter()
            .map(|link| link.link)
            .collect();
        assert!(!fields.is_empty());
        for cid in fields.iter().chain([&created.cid]) {
            let owners = db::docid::map::get_doc_ids_for_block(&systemstore, &cid.to_string())
                .await
                .unwrap();
            assert_eq!(owners, vec![created.doc_id.clone()], "{cid}");
        }
    })
    .unwrap()
}

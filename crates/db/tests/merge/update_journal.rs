//! Which merged blocks reach the update journal: a block that changed the
//! document, once, however often it is delivered.

use std::sync::Arc;

use blockstore::Blockstore as _;
use blockstore::DefraBlockstore;
use cid::Cid;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};
use defra_core::merge::BlockMetadata;
use document::{Document, NormalValue};
use query::fetcher::{DocFetcher, DocumentReader, DocumentUpdateOptions, DocumentUpdatePage};
use storage::RegolithStore;

use super::merge_handler_tests::{create_doc_locally, make_handler_with_schema_and_bus};

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

/// The first page of the `Users` journal, read by anyone.
pub(super) async fn journal(db: &Arc<DB<RegolithStore>>) -> DocumentUpdatePage {
    let acp = acp::LocalDocumentACP::new(Arc::new(acp::MemoryAcpStore::new()));
    db::LensedAutoCommitFetcher::new(db.clone())
        .get_document_updates(
            &DocumentUpdateOptions {
                collection: "Users".into(),
                after: 0,
                limit: 10,
            },
            &DocumentReader {
                context: &defra_core::RequestContext::default(),
                acp: &acp,
                projection: None,
                identity: &acp::Identity::Anonymous,
            },
        )
        .await
        .unwrap()
}

/// A peer's update of `name`, on top of the document's first block.
struct PeerUpdate {
    cid: Cid,
    block: Block,
    payload: CompositeDeltaPayload,
    field_cid: Cid,
    field: CrdtDelta,
}

impl PeerUpdate {
    async fn store(
        blockstore: &DefraBlockstore<RegolithStore>,
        created: &db::block::builder::BlockResult,
        name: &str,
        priority: u64,
    ) -> Self {
        let mut value = Vec::new();
        ciborium::into_writer(&NormalValue::String(name.into()), &mut value).unwrap();
        let delta = CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".into(),
            collection_version_id: "v1".into(),
            priority,
            data: value,
        });
        let field = Block::new(delta.clone(), created.field_cids.clone(), vec![]);
        let field_cid = field.generate_cid().unwrap();
        blockstore
            .put(&field_cid, &field.to_dag_cbor().unwrap())
            .await
            .unwrap();
        let payload = CompositeDeltaPayload {
            collection_version_id: "v1".into(),
            priority,
            status: 1,
        };
        let block = Block::new(
            CrdtDelta::Composite(payload.clone()),
            vec![created.cid],
            vec![DAGLink::new("name", field_cid)],
        );
        let cid = block.generate_cid().unwrap();
        blockstore
            .put(&cid, &block.to_dag_cbor().unwrap())
            .await
            .unwrap();
        Self {
            cid,
            block,
            payload,
            field_cid,
            field: delta,
        }
    }

    async fn merge(&self, handler: &Handler, doc_id: &str) {
        let metadata = BlockMetadata::normal(
            doc_id,
            "col-users",
            "did:key:z6MkrBatchMergeTest",
            None,
            false,
        );
        handler
            .process_composite_delta(&self.cid, &self.block, &self.payload, &metadata, false, 0)
            .await
            .unwrap();
    }
}

async fn document(handler: &Handler) -> (String, db::block::builder::BlockResult) {
    let collection = handler
        .db()
        .find_collection_by_id("col-users")
        .unwrap()
        .unwrap();
    let mut doc = Document::new(collection.schema_arc());
    doc.set("name", "before").unwrap();
    doc.set_collection_version_id("v1");
    let (doc_id, _, created) = create_doc_locally(handler, &collection, &mut doc).await;
    (doc_id.to_string(), created)
}

#[test]
fn a_merge_that_loses_to_the_stored_value_adds_no_row() {
    workers::block_on(async {
        let (handler, blockstore, _) = make_handler_with_schema_and_bus().await;
        let (id, created) = document(&handler).await;

        let winner = PeerUpdate::store(&blockstore, &created, "winner", 3).await;
        winner.merge(&handler, &id).await;
        // An older write of the same field, and one that loses the tie at the
        // winner's own priority: neither changes the document.
        for (name, priority) in [("older", 2), ("tie", 3)] {
            PeerUpdate::store(&blockstore, &created, name, priority)
                .await
                .merge(&handler, &id)
                .await;
        }

        let page = journal(handler.db()).await;
        assert_eq!(page.head, 1, "{:?}", page.entries);
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].doc_id, id);
        assert_eq!(page.entries[0].cid, winner.cid.to_string());
    })
    .unwrap();
}

#[test]
fn a_block_merged_again_by_another_handler_adds_no_second_row() {
    workers::block_on(async {
        let (handler, blockstore, _) = make_handler_with_schema_and_bus().await;
        let (id, created) = document(&handler).await;
        let update = PeerUpdate::store(&blockstore, &created, "after", 2).await;
        update.merge(&handler, &id).await;

        // A handler that has merged nothing yet runs the whole merge again,
        // where the first one answers a repeat from what it remembers.
        let restarted = DbMergeHandler::new(handler.db().clone(), blockstore.clone());
        update.merge(&restarted, &id).await;

        let page = journal(handler.db()).await;
        assert_eq!(page.head, 1, "{:?}", page.entries);
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].cid, update.cid.to_string());
    })
    .unwrap();
}

#[test]
fn a_block_whose_field_was_merged_on_its_own_first_still_gets_its_row() {
    workers::block_on(async {
        let (handler, blockstore, _) = make_handler_with_schema_and_bus().await;
        let (id, created) = document(&handler).await;
        let update = PeerUpdate::store(&blockstore, &created, "after", 2).await;

        // The field block arrives before the block that links it.
        let txn = handler.db().new_txn(false).await.unwrap();
        let doc_short_id = db::docid::map::get_doc_ref(&txn.systemstore().unwrap(), &id)
            .await
            .unwrap()
            .unwrap()
            .doc_short_id;
        let merged = handler
            .merge_field_in_txn(
                &mut txn.datastore().unwrap(),
                &txn.headstore().unwrap(),
                &update.field_cid,
                &update.field,
                Some("col-users"),
                &id,
                doc_short_id,
            )
            .await
            .unwrap();
        assert!(merged.applied());
        txn.force_commit().await.unwrap();
        assert_eq!(journal(handler.db()).await.head, 0);

        // Its field now only repeats the stored state, and the block is still
        // the update.
        update.merge(&handler, &id).await;
        let page = journal(handler.db()).await;
        assert_eq!(page.head, 1, "{:?}", page.entries);
        assert_eq!(page.entries[0].cid, update.cid.to_string());
    })
    .unwrap();
}

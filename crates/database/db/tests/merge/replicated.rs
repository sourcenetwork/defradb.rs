//! Replicated `Log` documents and their merges, shared by the merge tests
//! that judge what a replicated commit stores.

use std::sync::Arc;

use blockstore::DefraBlockstore;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};
use document::{DocID, Document, NormalValue};
use schema::CollectionVersion;
use storage::RegolithStore;

pub(super) type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

pub(super) const LOG_SDL: &str = "type Log { address: String blockNumber: Int }";
pub(super) const BRANCHABLE_LOG_SDL: &str =
    "type Log @branchable { address: String blockNumber: Int }";

pub(super) fn logs() -> CollectionVersion {
    crate::common::schema::parsed(LOG_SDL)
}

pub(super) fn log_version_id() -> &'static str {
    static ID: regolith::sync::Lazy<String> = regolith::sync::Lazy::new(|| logs().version_id);
    &ID
}

pub(super) fn log_collection_id() -> &'static str {
    static ID: regolith::sync::Lazy<String> = regolith::sync::Lazy::new(|| logs().collection_id);
    &ID
}

/// A replicated create of a Log whose `blockNumber` is `height`, or unset.
pub(super) async fn log(
    blockstore: &Arc<DefraBlockstore<RegolithStore>>,
    schema: &CollectionVersion,
    address: &str,
    height: Option<i64>,
) -> MergeBlock {
    let mut doc = Document::new(schema.clone());
    doc.set("address", NormalValue::String(address.to_string()))
        .unwrap();
    if let Some(height) = height {
        doc.set("blockNumber", NormalValue::Int(height)).unwrap();
    }
    let built = db::block::builder::build_blocks_from_document(
        &storage::Crdts::default(),
        &doc,
        schema,
        blockstore,
    )
    .await
    .unwrap();
    MergeBlock {
        cid: built.cid,
        block_data: built.block,
        doc_id: built.doc_id,
        collection_id: schema.collection_id.clone(),
        creator: "did:key:z6MkrReplicatedTest".to_string(),
        sender_peer: Some("peer1".to_string()),
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
        verified_creator: None,
    }
}

pub(super) async fn merge(handler: &Handler, block: &MergeBlock) -> MergeOutcome {
    let metadata = BlockMetadata::normal(
        &block.doc_id,
        &block.collection_id,
        &block.creator,
        block.sender_peer.as_deref(),
        false,
    );
    handler
        .handle_block(&block.cid, &block.block_data, metadata)
        .await
        .unwrap()
}

pub(super) async fn stored(handler: &Handler, collection_id: &str, doc_id: &str) -> bool {
    let collection = handler
        .db()
        .find_collection_by_id(collection_id)
        .unwrap()
        .unwrap();
    let txn = handler.db().new_txn(true).await.unwrap();
    let doc = {
        let datastore = txn.datastore().unwrap();
        let systemstore = txn.systemstore().unwrap();
        collection
            .get_by_doc_id(
                &datastore,
                &systemstore,
                &DocID::from_string(doc_id).unwrap(),
            )
            .await
            .unwrap()
    };
    txn.force_discard().unwrap();
    doc.is_some()
}

//! Replicated `Log` documents and their merges, shared by the merge tests
//! that judge what a replicated commit stores.

use std::sync::Arc;

use blockstore::DefraBlockstore;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};
use document::{DocID, Document, NormalValue};
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

pub(super) type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

pub(super) fn logs(collection_id: &str) -> CollectionVersion {
    CollectionVersion::new(
        "Log",
        "v1",
        collection_id,
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "2",
                "address",
                FieldKind::string(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "3",
                "blockNumber",
                FieldKind::int(),
                schema::CType::LwwRegister,
            ),
        ],
    )
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

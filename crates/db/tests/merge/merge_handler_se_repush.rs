//! A merged document's SE artifacts must be pushed to this node's replicators,
//! on both the standalone and the batch merge path.

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use blockstore::DefraBlockstore;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use db::merge::SeArtifactRepusher;
use defra_core::merge::BlockMetadata;
use defra_core::merge::MergeHandler;
use defra_core::merge::MergeOutcome;
use schema::CollectionVersion;
use schema::EncryptedIndexDescription;
use schema::FieldDescription;
use schema::FieldKind;
use storage::RegolithStore;

use super::merge_handler_tests::build_merge_block;

#[derive(Default)]
struct RecordingRepusher {
    calls: Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl SeArtifactRepusher for RecordingRepusher {
    async fn regenerate_and_push_se_artifacts(&self, collection_id: &str, doc_id: &str) {
        self.calls
            .lock()
            .unwrap()
            .push((collection_id.to_string(), doc_id.to_string()));
    }
}

async fn make_handler_with_encrypted_index() -> (
    DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>,
    Arc<DefraBlockstore<RegolithStore>>,
    Arc<RecordingRepusher>,
) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());

    let mut collection = CollectionVersion::new(
        "Users",
        "v1",
        "col-users",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "name", FieldKind::string()),
            FieldDescription::new("3", "age", FieldKind::int()),
        ],
    );
    collection.encrypted_indexes = vec![EncryptedIndexDescription::new("name")];
    db.create_collection(collection).await.unwrap();

    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    let handler = DbMergeHandler::new(db, blockstore.clone());
    let repusher = Arc::new(RecordingRepusher::default());
    handler.set_se_repusher(repusher.clone());
    (handler, blockstore, repusher)
}

#[tokio::test]
async fn standalone_merge_pushes_se_artifacts_for_the_merged_document() {
    let (handler, blockstore, repusher) = make_handler_with_encrypted_index().await;
    let block = build_merge_block(&blockstore, "Alice", 30).await;
    let doc_id = block.doc_id.clone();

    let metadata = BlockMetadata::normal(
        &block.doc_id,
        "col-users",
        &block.creator,
        block.sender_peer.as_deref(),
        false,
    );
    let outcome = handler
        .handle_block(&block.cid, &block.block_data, metadata)
        .await
        .unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert_eq!(
        *repusher.calls.lock().unwrap(),
        vec![("col-users".to_string(), doc_id)]
    );
}

#[tokio::test]
async fn batch_merge_pushes_se_artifacts_for_every_merged_document() {
    let (handler, blockstore, repusher) = make_handler_with_encrypted_index().await;
    let first = build_merge_block(&blockstore, "Alice", 30).await;
    let second = build_merge_block(&blockstore, "Bob", 31).await;
    let mut expected = vec![
        ("col-users".to_string(), first.doc_id.clone()),
        ("col-users".to_string(), second.doc_id.clone()),
    ];

    let results = handler.handle_block_batch(&[first, second]).await;

    assert!(results
        .iter()
        .all(|result| matches!(result, Ok(MergeOutcome::Merged))));
    let mut calls = repusher.calls.lock().unwrap().clone();
    calls.sort();
    expected.sort();
    assert_eq!(calls, expected);
}

#[tokio::test]
async fn multi_commit_dag_pushes_se_artifacts_once_for_the_head() {
    use blockstore::Blockstore as _;
    use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};

    let (handler, blockstore, repusher) = make_handler_with_encrypted_index().await;
    let create = build_merge_block(&blockstore, "Alice", 30).await;

    let mut data = Vec::new();
    ciborium::into_writer(&"Alicia", &mut data).unwrap();
    let field = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".to_string(),
            schema_version_id: "v1".to_string(),
            priority: 2,
            data,
        }),
        vec![],
        vec![],
    );
    let field_cid = field.generate_cid().unwrap();
    blockstore
        .put(&field_cid, &field.to_dag_cbor().unwrap())
        .await
        .unwrap();
    let update = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: "v1".to_string(),
            priority: 2,
            status: 1,
        }),
        vec![create.cid],
        vec![DAGLink::new("name", field_cid)],
    );
    let update_cid = update.generate_cid().unwrap();
    let update_data = update.to_dag_cbor().unwrap();
    blockstore.put(&update_cid, &update_data).await.unwrap();

    let metadata = BlockMetadata::normal(
        &create.doc_id,
        "col-users",
        &create.creator,
        create.sender_peer.as_deref(),
        false,
    );
    let outcome = handler
        .handle_block(&update_cid, &update_data, metadata)
        .await
        .unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert_eq!(
        *repusher.calls.lock().unwrap(),
        vec![("col-users".to_string(), create.doc_id.clone())],
        "one push for the head, not one per merged parent"
    );
}

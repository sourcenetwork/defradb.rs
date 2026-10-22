//! Regression coverage for merging collection blocks after collection locks
//! were replaced by transaction-local schema-key validation.

use blockstore::DefraBlockstore;
use db::block::builder::BlockResult;
use db::merge::merge_handler::{DbMergeHandler, MergeError};
use db::DB;
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};
use document::Document;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use std::future::Future;
use std::sync::Arc;
use storage::RegolithStore;

const CREATOR: &str = "did:key:merge-guard-test";

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

fn branchable_schema(version_id: &str, collection_id: &str) -> CollectionVersion {
    CollectionVersion::new(
        "GuardRace",
        version_id,
        collection_id,
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "body", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
    .as_branchable()
}

/// Merge one branchable document block and verify the transaction commits.
async fn merge_commits<M, F>(version_id: &str, collection_id: &'static str, merge: M)
where
    M: FnOnce(Handler, BlockResult) -> F + Send + 'static,
    F: Future<Output = Result<MergeOutcome, MergeError>> + Send + 'static,
{
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collection(branchable_schema(version_id, collection_id))
        .await
        .unwrap();

    let blockstore = Arc::new(DefraBlockstore::new(store.clone(), false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());
    let guard_race = db
        .get_collection("GuardRace")
        .unwrap()
        .unwrap()
        .schema_arc();
    let mut doc = Document::new(guard_race);
    doc.set("body", "hello".to_string()).unwrap();
    let built = db::block::builder::build_blocks_from_document(
        &doc,
        &crate::common::schema::version(version_id),
        &blockstore,
    )
    .await
    .unwrap();
    let outcome = merge(handler, built).await.expect("merge should succeed");
    assert!(matches!(outcome, MergeOutcome::Merged));
}

#[test]
fn merge_commits_with_transactional_schema_validation() {
    workers::block_on(async {
        merge_commits(
            "guard-race-v1",
            "col-guard-race",
            |handler, built| async move {
                let metadata =
                    BlockMetadata::normal(&built.doc_id, "col-guard-race", CREATOR, None, false);
                handler
                    .handle_block(&built.cid, &built.block, metadata)
                    .await
            },
        )
        .await;
    })
    .unwrap()
}

/// Metadata recovered from a block carries its schema version, which after
/// a patch is not the collection ID.
#[test]
fn recovered_merge_commits_with_transactional_schema_validation() {
    workers::block_on(async {
        merge_commits(
            "guard-race-recovered-v2",
            "col-guard-race-recovered",
            |handler, built| async move {
                let metadata = BlockMetadata::recovered(
                    &built.doc_id,
                    "guard-race-recovered-v2",
                    CREATOR,
                    None,
                );
                handler
                    .handle_block(&built.cid, &built.block, metadata)
                    .await
            },
        )
        .await;
    })
    .unwrap()
}

#[test]
fn batch_merge_commits_with_transactional_schema_validation() {
    workers::block_on(async {
        merge_commits(
            "guard-race-batch-v2",
            "col-guard-race-batch",
            |handler, built| async move {
                let mut results = handler
                    .handle_block_batch(&[MergeBlock {
                        cid: built.cid,
                        block_data: built.block,
                        doc_id: built.doc_id,
                        collection_id: "guard-race-batch-v2".to_string(),
                        creator: CREATOR.to_string(),
                        sender_peer: None,
                        is_explicit_replicator: false,
                        explicit_replay_authorization: None,
                        verified_creator: None,
                    }])
                    .await;
                results.remove(0)
            },
        )
        .await;
    })
    .unwrap()
}

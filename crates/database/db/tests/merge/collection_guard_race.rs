//! Regression coverage for merging collection blocks after collection locks
//! were replaced by transaction-local schema-key validation.

use blockstore::DefraBlockstore;
use db::block::builder::BlockResult;
use db::merge::merge_handler::{DbMergeHandler, MergeError};
use db::DB;
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};
use document::Document;
use std::future::Future;
use std::sync::Arc;
use storage::RegolithStore;

const CREATOR: &str = "did:key:merge-guard-test";

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

const GUARD_RACE_SDL: &str = "type GuardRace @branchable { body: String }";

/// The IDs of the version a test's block is written at.
struct Ids {
    version_id: String,
    collection_id: String,
}

/// Merge one branchable document block and verify the transaction commits.
/// With `patched`, the block is written at a second version, whose ID is not
/// the collection's.
async fn merge_commits<M, F>(patched: bool, merge: M)
where
    M: FnOnce(Handler, BlockResult, Ids) -> F + Send + 'static,
    F: Future<Output = Result<MergeOutcome, MergeError>> + Send + 'static,
{
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.add_collection(&defra_core::RequestContext::ambient(), GUARD_RACE_SDL)
        .await
        .unwrap();
    if patched {
        db.patch_collection(
            &defra_core::RequestContext::default(),
            r#"[{"op":"add","path":"/GuardRace/Fields/-","value":{"Name":"extra","Kind":"Int"}}]"#,
            None,
        )
        .await
        .unwrap();
    }

    let blockstore = Arc::new(DefraBlockstore::new(store.clone(), false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());
    let guard_race = db
        .get_collection("GuardRace")
        .unwrap()
        .unwrap()
        .schema_arc();
    let ids = Ids {
        version_id: guard_race.version_id.clone(),
        collection_id: guard_race.collection_id.clone(),
    };
    assert_eq!(patched, ids.version_id != ids.collection_id);
    let mut doc = Document::new(guard_race.clone());
    doc.set("body", "hello".to_string()).unwrap();
    let built = db::block::builder::build_blocks_from_document(
        &storage::Crdts::default(),
        &doc,
        &guard_race,
        &blockstore,
    )
    .await
    .unwrap();
    let outcome = merge(handler, built, ids)
        .await
        .expect("merge should succeed");
    assert!(matches!(outcome, MergeOutcome::Merged));
}

#[test]
fn merge_commits_with_transactional_schema_validation() {
    workers::block_on(async {
        merge_commits(false, |handler, built, ids| async move {
            let metadata =
                BlockMetadata::normal(&built.doc_id, &ids.collection_id, CREATOR, None, false);
            handler
                .handle_block(&built.cid, &built.block, metadata)
                .await
        })
        .await;
    })
    .unwrap()
}

/// Metadata recovered from a block carries its schema version, which after
/// a patch is not the collection ID.
#[test]
fn recovered_merge_commits_with_transactional_schema_validation() {
    workers::block_on(async {
        merge_commits(true, |handler, built, ids| async move {
            let metadata = BlockMetadata::recovered(&built.doc_id, &ids.version_id, CREATOR, None);
            handler
                .handle_block(&built.cid, &built.block, metadata)
                .await
        })
        .await;
    })
    .unwrap()
}

#[test]
fn batch_merge_commits_with_transactional_schema_validation() {
    workers::block_on(async {
        merge_commits(true, |handler, built, ids| async move {
            let mut results = handler
                .handle_block_batch(&[MergeBlock {
                    cid: built.cid,
                    block_data: built.block,
                    doc_id: built.doc_id,
                    collection_id: ids.version_id,
                    creator: CREATOR.to_string(),
                    sender_peer: None,
                    is_explicit_replicator: false,
                    explicit_replay_authorization: None,
                    verified_creator: None,
                }])
                .await;
            results.remove(0)
        })
        .await;
    })
    .unwrap()
}

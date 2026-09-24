//! A P2P merge must hold the collection's read guard for the lifetime of the
//! transaction it writes, the same guard a truncate/delete/patch holds
//! exclusively (`DB::collection_write_guards`). Without it, a truncate can
//! delete a document's heads and keys out from under an in-flight merge.
//!
//! The guard wait is observed through the trace events
//! `DB::collection_read_guard` emits, so each test proves the merge reached
//! the lock and was released by the drop, not merely that it was slow. The
//! same schedule runs for a single block, for a block whose metadata names
//! the schema version rather than the collection (what crash recovery
//! reconstructs from a block), and for a batch.

use blockstore::DefraBlockstore;
use db::block::builder::BlockResult;
use db::merge::merge_handler::{DbMergeHandler, MergeError};
use db::DB;
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};
use document::Document;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use std::future::Future;
use std::sync::Arc;
use storage::corekv::{IterOptions, Store};
use storage::RegolithStore;

use crate::common::guard_events::{recorder, READ_HOLDING, READ_WAITING};

const CREATOR: &str = "did:key:merge-guard-test";

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

fn branchable_schema(version_id: &str, collection_id: &str) -> CollectionVersion {
    CollectionVersion::new(
        "GuardRace",
        version_id,
        collection_id,
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "body", FieldKind::string()),
        ],
    )
    .as_branchable()
}

async fn total_key_count(store: &Arc<RegolithStore>) -> usize {
    let txn = store.new_txn(true).await.unwrap();
    let mut iter = txn.iterator(IterOptions::new()).await.unwrap();
    let mut count = 0;
    while iter.next().await.unwrap().is_some() {
        count += 1;
    }
    count
}

/// Hold the collection write guard, run `merge` on a task, and prove it
/// waits on the guard, writes nothing meanwhile, and merges once the guard
/// is dropped. The guard events are counted per collection, so every
/// caller passes a collection id of its own.
async fn merge_waits_on_the_guard<M, F>(version_id: &str, collection_id: &'static str, merge: M)
where
    M: FnOnce(Handler, BlockResult) -> F + Send + 'static,
    F: Future<Output = Result<MergeOutcome, MergeError>> + Send + 'static,
{
    let recorder = recorder();
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collection(branchable_schema(version_id, collection_id))
        .await
        .unwrap();

    let blockstore = Arc::new(DefraBlockstore::new(store.clone(), false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());

    let mut doc = Document::new();
    doc.set("body", "hello".to_string());
    let built = db::block::builder::build_blocks_from_document(&doc, version_id, &blockstore)
        .await
        .unwrap();

    let baseline = total_key_count(&store).await;

    // Stand in for a concurrent truncate/delete/patch, which holds this same
    // guard exclusively for the duration of its own write.
    let guards = db
        .collection_write_guards(std::iter::once(collection_id.to_string()))
        .await
        .unwrap();

    let mut task = tokio::spawn(merge(handler, built));

    // Yield until the merge is observably waiting on the guard, then keep
    // yielding for a bounded window in which it must not acquire it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while recorder.count(collection_id, READ_WAITING) == 0 {
        if task.is_finished() {
            let outcome = (&mut task).await;
            panic!("the merge ended without waiting on the collection guard: {outcome:?}");
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the merge never reached the collection guard"
        );
        tokio::task::yield_now().await;
    }
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }

    assert_eq!(
        recorder.count(collection_id, READ_WAITING),
        1,
        "the merge must wait on the collection guard exactly once"
    );
    assert_eq!(
        recorder.count(collection_id, READ_HOLDING),
        0,
        "the merge must not acquire the collection guard while a \
         truncate-equivalent write guard is held"
    );
    assert!(!task.is_finished());
    assert_eq!(
        total_key_count(&store).await,
        baseline,
        "merge must not write any head or document key while the collection \
         guard is held"
    );

    drop(guards);

    let outcome = task
        .await
        .expect("merge task panicked")
        .expect("merge should succeed once the guard is released");
    assert_eq!(
        recorder.count(collection_id, READ_HOLDING),
        1,
        "the merge must acquire the collection guard once the drop released it"
    );
    assert!(
        matches!(outcome, MergeOutcome::Merged),
        "expected the composite to merge, got {outcome:?}"
    );
    assert!(
        total_key_count(&store).await > baseline,
        "merge must land its head and document keys once unblocked"
    );
}

#[tokio::test]
async fn merge_blocks_on_a_held_collection_write_guard() {
    merge_waits_on_the_guard(
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
}

/// Metadata recovered from a block carries its schema version, which after
/// a patch is not the collection id. The guard is keyed by the collection,
/// so the merge has to resolve one from the other before it can wait.
#[tokio::test]
async fn recovered_merge_blocks_on_a_held_collection_write_guard() {
    merge_waits_on_the_guard(
        "guard-race-recovered-v2",
        "col-guard-race-recovered",
        |handler, built| async move {
            let metadata =
                BlockMetadata::recovered(&built.doc_id, "guard-race-recovered-v2", CREATOR, None);
            handler
                .handle_block(&built.cid, &built.block, metadata)
                .await
        },
    )
    .await;
}

#[tokio::test]
async fn batch_merge_blocks_on_a_held_collection_write_guard() {
    merge_waits_on_the_guard(
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
}

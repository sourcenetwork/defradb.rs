mod fixture;

use blockstore::{Blockstore as _, DefraBlockstore};
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::NormalValue;
use std::sync::Arc;
use storage::RegolithStore;

use fixture::{converge, make_handler, make_handler_on, merge_turn, read_document, History};

async fn history_converges(revisions: u64, batch: bool) {
    let handler = make_handler().await;
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), revisions, "revision")
        .await;
    let root = history.root();
    converge(&handler, root, batch).await;
    let document = read_document(&handler, root).await.unwrap();
    assert_eq!(
        document.get("name"),
        Some(&NormalValue::String(format!("revision-{revisions}")))
    );
    assert_eq!(
        document.get("score"),
        Some(&NormalValue::Int(revisions as i64))
    );
}

#[tokio::test]
async fn fresh_receiver_merges_twelve_thousand_revisions() {
    history_converges(12_000, false).await;
}

#[tokio::test]
async fn batch_fresh_receiver_merges_twelve_thousand_revisions() {
    history_converges(12_000, true).await;
}

#[tokio::test]
async fn short_history_control() {
    history_converges(64, false).await;
    history_converges(64, true).await;
}

#[tokio::test]
async fn restarting_the_handler_preserves_progress_and_counter_values() {
    let initial = make_handler().await;
    let db = initial.db().clone();
    let blocks = initial.blockstore().clone();
    drop(initial);
    let mut history = History::default();
    history.append(blocks.as_ref(), 128, "revision").await;
    let root = history.root();
    let mut turns = 0;
    loop {
        let handler = DbMergeHandler::new_with_max_merge_depth(db.clone(), blocks.clone(), 8);
        let outcome = merge_turn(&handler, root, turns % 2 == 0).await;
        turns += 1;
        assert!(turns < 256, "restarting must not reset traversal progress");
        if outcome.is_merged() || outcome.is_terminal_skip() {
            let document = read_document(&handler, root).await.unwrap();
            assert_eq!(document.get("score"), Some(&NormalValue::Int(128)));
            break;
        }
        assert!(matches!(outcome, MergeOutcome::Yielded));
    }
    assert!(turns > 1, "the fixture must exercise a suspended traversal");
    let handler = DbMergeHandler::new_with_max_merge_depth(db, blocks, 8);
    converge(&handler, root, true).await;
    assert_eq!(
        read_document(&handler, root).await.unwrap().get("score"),
        Some(&NormalValue::Int(128)),
        "replaying committed history must not apply counter deltas twice"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_head_can_converge_while_older_history_is_suspended() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), 128, "revision")
        .await;
    let older = history.root().clone();
    let outcome = merge_turn(&handler, &older, false).await;
    assert!(matches!(outcome, MergeOutcome::Yielded));
    history
        .append(handler.blockstore().as_ref(), 1, "new-head")
        .await;
    let newer = history.root();
    tokio::join!(
        converge(&handler, &older, false),
        converge(&handler, newer, true)
    );
    let document = read_document(&handler, newer).await.unwrap();
    assert_eq!(
        document.get("name"),
        Some(&NormalValue::String("new-head-129".into()))
    );
    assert_eq!(document.get("score"), Some(&NormalValue::Int(129)));
}

#[tokio::test]
async fn disk_reopen_resumes_a_partially_applied_history() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(RegolithStore::open(directory.path()).unwrap());
    let initial = make_handler_on(store).await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    drop(initial);
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), 128, "revision")
        .await;
    let root = history.root();
    let mut interrupted = false;
    for _ in 0..128 {
        let outcome = merge_turn(&handler, root, true).await;
        if let Some(document) = read_document(&handler, root).await {
            let Some(NormalValue::Int(score)) = document.get("score") else {
                panic!("partially applied history must expose its committed counter");
            };
            if *score > 0 && *score < 128 {
                assert!(matches!(outcome, MergeOutcome::Yielded));
                interrupted = true;
                break;
            }
        }
        assert!(
            !outcome.is_merged(),
            "fixture must stop between history chunks"
        );
    }
    assert!(interrupted);
    drop(handler);

    let store = Arc::new(RegolithStore::open(directory.path()).unwrap());
    let db = Arc::new(
        db::database::DB::open_from_arc(store.clone())
            .await
            .unwrap(),
    );
    let handler = DbMergeHandler::new_with_max_merge_depth(
        db,
        Arc::new(DefraBlockstore::new(store, true)),
        8,
    );
    converge(&handler, root, true).await;
    let document = read_document(&handler, root).await.unwrap();
    assert_eq!(document.get("score"), Some(&NormalValue::Int(128)));
    assert_eq!(
        document.get("name"),
        Some(&NormalValue::String("revision-128".into()))
    );
}

#[tokio::test]
async fn shared_ancestors_are_applied_once_across_forks() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        2,
    );
    let mut left = History::default();
    left.append(handler.blockstore().as_ref(), 3, "shared")
        .await;
    let mut right = left.clone();
    left.append(handler.blockstore().as_ref(), 3, "left").await;
    right
        .append(handler.blockstore().as_ref(), 2, "right")
        .await;
    let block = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: "v1".into(),
            priority: 7,
            status: 1,
        }),
        vec![left.root().cid, right.root().cid],
        vec![],
    );
    let mut root = left.root().clone();
    root.cid = block.generate_cid().unwrap();
    root.block_data = block.to_dag_cbor().unwrap().into();
    handler
        .blockstore()
        .put(&root.cid, &root.block_data)
        .await
        .unwrap();
    converge(&handler, &root, true).await;
    let document = read_document(&handler, &root).await.unwrap();
    // Equal increments with the same nonce and parent share their field CID.
    assert_eq!(document.get("score"), Some(&NormalValue::Int(6)));
    assert_eq!(
        document.get("name"),
        Some(&NormalValue::String("left-6".into()))
    );
}

#[tokio::test]
async fn a_missing_ancestor_stays_retryable_until_restored() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), 64, "revision")
        .await;
    let root = history.root();
    let mut missing = root.cid;
    for _ in 0..32 {
        let bytes = handler.blockstore().get(&missing).await.unwrap().unwrap();
        missing = Block::from_dag_cbor(&bytes).unwrap().heads.unwrap()[0];
    }
    let saved = handler.blockstore().get(&missing).await.unwrap().unwrap();
    handler.blockstore().delete(&missing).await.unwrap();
    let mut stopped = false;
    for _ in 0..64 {
        let outcome = handler
            .handle_block(
                &root.cid,
                &root.block_data,
                BlockMetadata::normal(
                    &root.doc_id,
                    &root.collection_id,
                    &root.creator,
                    root.sender_peer.as_deref(),
                    false,
                ),
            )
            .await;
        match outcome {
            Ok(MergeOutcome::Yielded) => {}
            Err(error) => {
                assert_eq!(
                    handler.error_disposition(&error),
                    defra_core::merge::MergeErrorDisposition::Retryable
                );
                stopped = true;
                break;
            }
            other => panic!("missing history must not complete or be quarantined: {other:?}"),
        }
    }
    assert!(stopped);
    handler.blockstore().put(&missing, &saved).await.unwrap();
    converge(&handler, root, true).await;
    assert_eq!(
        read_document(&handler, root).await.unwrap().get("score"),
        Some(&NormalValue::Int(64))
    );
}

#[tokio::test]
async fn document_and_collection_deliveries_share_history_progress() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), 64, "revision")
        .await;
    let root = history.root();
    let payload = defra_core::block::CollectionDeltaPayload {
        schema_version_id: "v1".into(),
        priority: 1,
    };
    let collection = Block::new(
        CrdtDelta::Collection(payload.clone()),
        vec![],
        vec![defra_core::block::DAGLink::new(&root.doc_id, root.cid)],
    );
    let cid = collection.generate_cid().unwrap();
    let mut complete = false;
    for _ in 0..128 {
        merge_turn(&handler, root, false).await;
        let outcome = handler
            .process_collection_delta(
                &cid,
                &collection,
                &payload,
                &BlockMetadata::normal(
                    "",
                    "col-users",
                    "history-peer",
                    Some("another-provider"),
                    false,
                ),
                0,
            )
            .await
            .unwrap();
        if outcome.is_merged() || outcome.is_terminal_skip() {
            complete = true;
            break;
        }
        assert!(matches!(outcome, MergeOutcome::Yielded));
    }
    assert!(
        complete,
        "duplicate delivery must not keep resetting progress"
    );
    converge(&handler, root, true).await;
    assert_eq!(
        read_document(&handler, root).await.unwrap().get("score"),
        Some(&NormalValue::Int(64))
    );
}

#[tokio::test]
async fn completed_history_releases_its_admission_slot() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    let mut first = None;
    for index in 0..64 {
        let mut history = History::default();
        history
            .append(
                handler.blockstore().as_ref(),
                16,
                &format!("document-{index}"),
            )
            .await;
        assert_eq!(
            merge_turn(&handler, history.root(), false).await,
            MergeOutcome::Yielded
        );
        first.get_or_insert_with(|| history.root().clone());
    }
    let mut waiting = History::default();
    waiting
        .append(handler.blockstore().as_ref(), 16, "waiting")
        .await;
    assert!(matches!(
        merge_turn(&handler, waiting.root(), false).await,
        MergeOutcome::Skipped {
            terminal: false,
            ..
        }
    ));
    converge(&handler, &first.unwrap(), false).await;
    converge(&handler, waiting.root(), true).await;
    assert_eq!(
        read_document(&handler, waiting.root())
            .await
            .unwrap()
            .get("score"),
        Some(&NormalValue::Int(16))
    );
}

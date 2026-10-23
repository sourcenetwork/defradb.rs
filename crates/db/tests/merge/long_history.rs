mod admission;
mod deferred;
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
    workers::timeout(
        std::time::Duration::from_secs(120),
        check_history(revisions, batch),
    )
    .await
    .expect("history must converge without waiting for another P2P delivery");
}

async fn check_history(revisions: u64, batch: bool) {
    let handler = make_handler().await;
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), revisions, "revision")
        .await;
    let root = history.root();
    let outcome = if batch {
        handler
            .handle_block_batch(std::slice::from_ref(root))
            .await
            .remove(0)
            .unwrap()
    } else {
        handler
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
            .await
            .unwrap()
    };
    assert!(
        outcome.is_merged() || outcome.is_terminal_skip(),
        "{outcome:?}"
    );
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

#[test]
fn fresh_receiver_merges_twelve_thousand_revisions() {
    workers::block_on(async {
        history_converges(12_000, false).await;
    })
    .unwrap()
}

#[test]
fn batch_fresh_receiver_merges_twelve_thousand_revisions() {
    workers::block_on(async {
        history_converges(12_000, true).await;
    })
    .unwrap()
}

#[test]
fn short_history_control() {
    workers::block_on(async {
        history_converges(64, false).await;
        history_converges(64, true).await;
    })
    .unwrap()
}

#[test]
fn restarting_the_handler_preserves_progress_and_counter_values() {
    workers::block_on(async {
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
    })
    .unwrap()
}

#[test]
fn a_new_head_can_converge_while_older_history_is_suspended() {
    workers::block_on(async {
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
        futures::join!(
            converge(&handler, &older, false),
            converge(&handler, newer, true)
        );
        let document = read_document(&handler, newer).await.unwrap();
        assert_eq!(
            document.get("name"),
            Some(&NormalValue::String("new-head-129".into()))
        );
        assert_eq!(document.get("score"), Some(&NormalValue::Int(129)));
    })
    .unwrap()
}

#[test]
fn disk_reopen_resumes_a_partially_applied_history() {
    workers::block_on(async {
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
    })
    .unwrap()
}

#[test]
fn finishing_an_older_walk_does_not_resurrect_its_head() {
    workers::block_on(async {
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
        let older = history.root().clone();
        assert_eq!(
            merge_turn(&handler, &older, false).await,
            MergeOutcome::Yielded
        );
        history
            .append(handler.blockstore().as_ref(), 1, "newer")
            .await;
        converge(&handler, history.root(), true).await;

        let restarted = DbMergeHandler::new_with_max_merge_depth(
            handler.db().clone(),
            handler.blockstore().clone(),
            8,
        );
        converge(&restarted, &older, false).await;
        let txn = handler.db().new_txn(true).await.unwrap();
        let system = txn.systemstore().unwrap();
        let doc_ref = db::docid::map::get_doc_ref(&system, &older.doc_id)
            .await
            .unwrap()
            .unwrap();
        let headstore = txn.headstore().unwrap();
        let heads = db::block::heads::document::load(&headstore, doc_ref.doc_short_id)
            .await
            .unwrap();
        let composites: Vec<_> = heads
            .live()
            .filter_map(|(name, _)| {
                db::block::heads::document::parse_name(name)
                    .filter(|(field, _)| *field == "C")
                    .map(|(_, cid)| cid)
            })
            .collect();
        assert_eq!(
            composites,
            vec![history.root().cid],
            "a completed descendant must remain the only head"
        );
    })
    .unwrap()
}

#[test]
fn shared_ancestors_are_applied_once_across_forks() {
    workers::block_on(async {
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
    })
    .unwrap()
}

#[test]
fn a_missing_ancestor_stays_retryable_until_restored() {
    workers::block_on(async {
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
    })
    .unwrap()
}

#[test]
fn document_and_collection_deliveries_share_history_progress() {
    workers::block_on(async {
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
                .with_single_history_turn()
                .handle_block(
                    &cid,
                    &collection.to_dag_cbor().unwrap(),
                    BlockMetadata::normal(
                        "",
                        "col-users",
                        "history-peer",
                        Some("another-provider"),
                        false,
                    ),
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
    })
    .unwrap()
}

#[test]
fn completed_history_releases_its_admission_slot() {
    workers::block_on(async {
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
            let mut root = history.root().clone();
            root.sender_peer = Some(format!("peer-{}", index / 16));
            assert_eq!(
                merge_turn(&handler, &root, false).await,
                MergeOutcome::Yielded
            );
            first.get_or_insert(root);
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
    })
    .unwrap()
}

#[test]
fn idle_cursor_reclamation_preserves_blocks_and_allows_the_history_to_restart() {
    workers::block_on(async {
        let initial = make_handler().await;
        let handler = DbMergeHandler::new_with_max_merge_depth(
            initial.db().clone(),
            initial.blockstore().clone(),
            8,
        );
        let mut histories = Vec::new();
        for index in 0..64 {
            let mut history = History::default();
            history
                .append(handler.blockstore().as_ref(), 16, &format!("idle-{index}"))
                .await;
            let mut root = history.root().clone();
            root.sender_peer = Some(format!("peer-{}", index / 16));
            assert_eq!(
                merge_turn(&handler, &root, false).await,
                MergeOutcome::Yielded
            );
            histories.push(history);
        }
        let idle = histories[0].root();
        let txn = handler.db().new_txn(false).await.unwrap();
        let system = txn.systemstore().unwrap();
        let key = format!("/merge-history/v1/{}/state", idle.cid).into_bytes();
        let mut progress: serde_json::Value =
            serde_json::from_slice(&system.get(&key).await.unwrap().unwrap()).unwrap();
        progress["last_active_unix"] = serde_json::json!(1);
        system
            .set(&key, &serde_json::to_vec(&progress).unwrap())
            .await
            .unwrap();
        drop(system);
        txn.force_commit().await.unwrap();

        let mut waiting = History::default();
        waiting
            .append(handler.blockstore().as_ref(), 16, "waiting")
            .await;
        converge(&handler, waiting.root(), false).await;
        assert!(handler.blockstore().has(&idle.cid).await.unwrap());
        converge(&handler, idle, false).await;
        assert_eq!(
            read_document(&handler, idle).await.unwrap().get("score"),
            Some(&NormalValue::Int(16))
        );
    })
    .unwrap()
}

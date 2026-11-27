use std::sync::Arc;

use super::fixture::{
    converge, make_handler, merge_turn, read_document, users_version_id, Handler, History,
};
use blockstore::{Blockstore, DefraBlockstore};
use db::database::{DbOptions, DB};
use db::merge::merge_handler::DbMergeHandler;
use db::replication_filter::{FieldValues, ReplicationFilter};
use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta};
use defra_core::merge::{MergeBlock, MergeOutcome};
use document::NormalValue;
use storage::RegolithStore;

struct Bookkeeping {
    finished: bool,
    top: u64,
    accounted: Option<u64>,
    keys_known: bool,
    stored: u64,
    skipped: u64,
}

struct RefuseRevision;

impl ReplicationFilter for RefuseRevision {
    fn allow_replication(
        &self,
        _collection_id: &str,
        _collection_name: &str,
        _doc_id: &str,
        fields: &FieldValues,
    ) -> bool {
        fields.get("name").and_then(|value| value.as_text()) != Some("revision-3")
    }
}

async fn bookkeeping(handler: &Handler, root: &cid::Cid) -> Option<Bookkeeping> {
    let txn = handler.db().new_txn(true).await.unwrap();
    let store = txn.systemstore().unwrap();
    let state = store
        .get(format!("/merge-history/v1/{root}/state").as_bytes())
        .await
        .unwrap()?;
    let state: serde_json::Value = serde_json::from_slice(&state).unwrap();
    let mut entries = store
        .iterator(
            storage::corekv::IterOptions::new()
                .with_prefix(format!("/merge-history/v1/{root}/data/").into_bytes()),
        )
        .await
        .unwrap();
    let mut stored = 0;
    let mut skipped = 0;
    while let Some(entry) = entries.next().await.unwrap() {
        stored += 1;
        if entry.value.as_ref() == [1] {
            skipped += 1;
        }
    }
    entries.close().await.unwrap();
    Some(Bookkeeping {
        finished: !state["outcome"].is_null(),
        top: state["top"].as_u64().unwrap(),
        accounted: state["keys"].as_u64(),
        keys_known: state["keys_known"].as_bool().unwrap_or(false),
        stored,
        skipped,
    })
}

/// Walks `root` turn by turn and returns the deepest stack it reached.
async fn walk_accounted(handler: &Handler, root: &MergeBlock, only_open: bool) -> u64 {
    let mut deepest = 0;
    for _ in 0..1024 {
        let outcome = merge_turn(handler, root, false).await;
        if outcome.is_merged() {
            assert!(bookkeeping(handler, &root.cid).await.is_none());
            return deepest;
        }
        assert_eq!(outcome, MergeOutcome::Yielded);
        let Some(state) = bookkeeping(handler, &root.cid).await else {
            continue;
        };
        assert_eq!(
            state.accounted.unwrap(),
            state.stored,
            "the root's progress must account every bookkeeping key it holds"
        );
        let open = if state.finished { 0 } else { state.top + 1 };
        if only_open {
            assert_eq!(
                state.stored,
                2 * open,
                "only the open stack may hold bookkeeping: a frame and a mark per level"
            );
        }
        deepest = deepest.max(state.top);
    }
    panic!("history made no bounded progress");
}

#[test]
fn large_history_bookkeeping_is_accounted_and_retains_only_the_open_stack() {
    workers::block_on(async {
        let initial = make_handler().await;
        let handler = DbMergeHandler::new_with_max_merge_depth(
            initial.db().clone(),
            initial.blockstore().clone(),
            8,
        );
        let mut history = History::default();
        history
            .append(handler.blockstore().as_ref(), 300, "first")
            .await;
        assert!(
            walk_accounted(&handler, history.root(), true).await >= 280,
            "the fixture must hold most of the history on the stack at once"
        );

        history
            .append(handler.blockstore().as_ref(), 300, "second")
            .await;
        assert!(
            walk_accounted(&handler, history.root(), true).await <= 300,
            "a walk stops at the applied history below it"
        );
        converge(&handler, history.root(), false).await;
        assert_eq!(
            read_document(&handler, history.root())
                .await
                .unwrap()
                .get("score"),
            Some(&NormalValue::Int(600))
        );
    })
    .unwrap()
}

#[test]
fn forked_history_accounting_covers_a_shared_ancestor() {
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
                collection_version_id: users_version_id().to_string(),
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

        walk_accounted(&handler, &root, true).await;
        assert_eq!(
            read_document(&handler, &root).await.unwrap().get("score"),
            Some(&NormalValue::Int(6))
        );
    })
    .unwrap()
}

#[test]
fn a_terminal_skip_retains_its_mark_and_remains_accounted() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let options = DbOptions::new().with_replication_filter(Arc::new(RefuseRevision));
        let db = Arc::new(DB::from_arc_with_options(store.clone(), options).unwrap());
        db.add_collection(
            &defra_core::RequestContext::ambient(),
            super::fixture::USERS_SDL,
        )
        .await
        .unwrap();
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let handler = DbMergeHandler::new_with_max_merge_depth(db, blockstore, 2);
        let mut history = History::default();
        history
            .append(handler.blockstore().as_ref(), 8, "revision")
            .await;
        let root = history.root();

        let mut saw_skipped = false;
        for _ in 0..128 {
            let outcome = merge_turn(&handler, root, false).await;
            if outcome.is_merged() {
                break;
            }
            assert_eq!(outcome, MergeOutcome::Yielded);
            let Some(state) = bookkeeping(&handler, &root.cid).await else {
                continue;
            };
            assert_eq!(state.accounted.unwrap(), state.stored);
            saw_skipped |= state.skipped > 0;
        }
        assert!(saw_skipped, "the filter must terminally skip one ancestor");
        assert!(read_document(&handler, root).await.is_some());
    })
    .unwrap()
}

#[test]
fn a_state_without_the_key_count_restarts_instead_of_resuming_from_zero() {
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
        assert_eq!(
            merge_turn(&handler, root, false).await,
            MergeOutcome::Yielded
        );

        let txn = handler.db().new_txn(false).await.unwrap();
        let store = txn.systemstore().unwrap();
        let key = format!("/merge-history/v1/{}/state", root.cid).into_bytes();
        let mut state: serde_json::Value =
            serde_json::from_slice(&store.get(&key).await.unwrap().unwrap()).unwrap();
        let state = state.as_object_mut().unwrap();
        state.remove("keys");
        state.remove("keys_known");
        store
            .set(&key, &serde_json::to_vec(&state).unwrap())
            .await
            .unwrap();
        drop(store);
        txn.force_commit().await.unwrap();

        let mut restarted = false;
        for _ in 0..1024 {
            let outcome = merge_turn(&handler, root, false).await;
            if outcome.is_merged() {
                break;
            }
            assert_eq!(outcome, MergeOutcome::Yielded);
            let Some(state) = bookkeeping(&handler, &root.cid).await else {
                continue;
            };
            if !state.keys_known {
                continue;
            }
            restarted = true;
            assert_eq!(
                state.accounted.unwrap(),
                state.stored,
                "the restarted walk must account every bookkeeping key it holds"
            );
        }
        assert!(
            restarted,
            "the walk must restart and resume with a known key count"
        );
        assert_eq!(
            read_document(&handler, root).await.unwrap().get("score"),
            Some(&NormalValue::Int(64))
        );
    })
    .unwrap()
}

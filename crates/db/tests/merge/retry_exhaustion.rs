//! Bounded merge retries (`go.shinzo.merge.retry`): when every attempt
//! conflicts the event is dropped with Go's `reached maximum transaction`
//! text, and a budget of zero still makes one attempt.

use std::sync::Arc;

use blockstore::DefraBlockstore;
use db::database::{DbOptions, DB};
use db::merge::merge_handler::DbMergeHandler;
use db::merge::AcpMergeHandler;
use defra_core::merge::{BlockMetadata, MergeHandler};

use super::retention::{log, logs};

/// An event whose every commit attempt conflicted, as the network sees it:
/// the engine's exhaustion under Go's drop wrapper.
#[test]
#[mantra_macros::req_test("go.shinzo.merge.retry")]
fn exhausted_retries_drop_the_event_with_gos_text() {
    let conflict = storage::corekv::TransactionConflict {
        access: regolith::Access::Read,
        winning_write: regolith::WriteKind::Put,
        observed_seq: 4,
        latest_seq: 5,
    };
    let exhausted = db::merge::merge_handler::MergeError::Database(db::error::Error::Storage(
        storage::Error::TxnRetryExhausted(conflict),
    ));
    let cid = defra_core::block::generate_cid_from_bytes(b"head").unwrap();
    let text = exhausted.dropped("bae-doc", &cid).to_string();
    assert!(text.starts_with("merge event dropped: "), "{text}");
    assert!(
        text.contains("reached maximum transaction reties: "),
        "{text}"
    );
    assert!(
        text.ends_with(&format!(". DocID: bae-doc, CID: {cid}")),
        "{text}"
    );
}

#[test]
#[mantra_macros::req_test("go.shinzo.merge.retry")]
fn a_budget_of_zero_still_makes_one_attempt() {
    workers::block_on(async {
        let store = Arc::new(storage::RegolithStore::in_memory().unwrap());
        let db = Arc::new(
            DB::from_arc_with_options(store.clone(), DbOptions::new().with_max_txn_retries(0))
                .unwrap(),
        );
        db.create_collection(logs("col-log")).await.unwrap();
        let blockstore = Arc::new(DefraBlockstore::new(store, false));
        let handler = AcpMergeHandler::new(Arc::new(DbMergeHandler::new(db, blockstore.clone())));
        let block = log(&blockstore, &logs("col-log"), "once", Some(1)).await;
        let metadata = BlockMetadata::normal(
            &block.doc_id,
            &block.collection_id,
            &block.creator,
            None,
            false,
        );
        let merged = handler
            .handle_block(&block.cid, &block.block_data, metadata)
            .await;
        assert_eq!(
            merged.unwrap(),
            defra_core::merge::MergeOutcome::Merged,
            "no retries is one attempt, not none"
        );
    })
    .unwrap()
}

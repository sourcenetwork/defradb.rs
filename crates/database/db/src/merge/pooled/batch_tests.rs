//! A batch that spans documents is split by document: each part merges on
//! its document's worker, and the outcomes come back in the batch's order.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};

use super::PooledMergeHandler;

#[derive(Debug)]
struct NotRun(workers::Error);

impl std::fmt::Display for NotRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for NotRun {}

impl From<workers::Error> for NotRun {
    fn from(error: workers::Error) -> Self {
        Self(error)
    }
}

/// Answers each block with its document and the worker it merged on.
#[derive(Default)]
struct WhereMerged {
    parts: AtomicUsize,
}

#[async_trait]
impl MergeHandler for WhereMerged {
    type Error = NotRun;

    async fn handle_block(
        &self,
        _cid: &Cid,
        _block_data: &[u8],
        _metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, NotRun> {
        Ok(MergeOutcome::Merged)
    }

    async fn handle_block_batch(&self, blocks: &[MergeBlock]) -> Vec<Result<MergeOutcome, NotRun>> {
        self.parts.fetch_add(1, Ordering::SeqCst);
        let worker = workers::current_worker().map(|id| id.index);
        blocks
            .iter()
            .map(|block| {
                Ok(MergeOutcome::Skipped {
                    reason: format!("{} {worker:?}", block.doc_id),
                    terminal: false,
                })
            })
            .collect()
    }
}

fn block(doc: &str) -> MergeBlock {
    MergeBlock {
        cid: Cid::default(),
        block_data: Bytes::new(),
        doc_id: doc.into(),
        collection_id: "c1".into(),
        creator: String::new(),
        sender_peer: None,
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
        verified_creator: None,
    }
}

#[test]
fn a_batch_spanning_documents_merges_each_on_its_worker_in_order() {
    let pool = workers::Pool::start(workers::Config::unplaced(4)).unwrap();
    let inner = Arc::new(WhereMerged::default());
    let handler = PooledMergeHandler::new(Arc::clone(&inner), Some(pool.clone()));
    let docs = ["bae-1", "bae-2", "bae-1", "bae-3", "bae-2"];
    let blocks: Vec<MergeBlock> = docs.iter().map(|doc| block(doc)).collect();

    let outcomes =
        workers::block_on_timeout(handler.handle_block_batch(&blocks), Duration::from_secs(10))
            .unwrap();

    let merged: Vec<(String, String)> = outcomes
        .into_iter()
        .map(|outcome| match outcome.unwrap() {
            MergeOutcome::Skipped { reason, .. } => {
                let (doc, worker) = reason.split_once(' ').unwrap();
                (doc.to_owned(), worker.to_owned())
            }
            other => panic!("unexpected outcome {other:?}"),
        })
        .collect();
    let in_order: Vec<&str> = merged.iter().map(|(doc, _)| doc.as_str()).collect();
    assert_eq!(in_order, docs, "outcomes come back in the batch's order");
    assert_eq!(
        inner.parts.load(Ordering::SeqCst),
        3,
        "one part per document"
    );
    for (doc, worker) in &merged {
        assert_ne!(worker, "None", "{doc} merged off the pool");
        let same: Vec<_> = merged.iter().filter(|(other, _)| other == doc).collect();
        assert!(
            same.iter().all(|(_, other)| other == worker),
            "{doc}'s blocks merged on one worker"
        );
    }
    pool.shutdown_blocking().unwrap();
}

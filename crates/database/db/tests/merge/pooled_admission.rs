//! A merge waits for an admission slot no longer than its deadline, and a
//! wait that is dropped leaves no slot taken.

use std::future::Future;
use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Context;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};
use regolith::sync::Event;

use super::PooledMergeHandler;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct NotRun(workers::Error);

impl From<workers::Error> for NotRun {
    fn from(error: workers::Error) -> Self {
        Self(error)
    }
}

/// Holds every merge until `release` is set.
#[derive(Default)]
struct Held {
    release: Event,
    merged: AtomicUsize,
}

#[async_trait]
impl MergeHandler for Held {
    type Error = NotRun;

    async fn handle_block(
        &self,
        _cid: &Cid,
        _block_data: &[u8],
        _metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, NotRun> {
        self.release.wait().await;
        self.merged.fetch_add(1, Ordering::SeqCst);
        Ok(MergeOutcome::Merged)
    }
}

fn merge<'a>(
    handler: &'a PooledMergeHandler<Held>,
    cid: &'a Cid,
) -> impl Future<Output = Result<MergeOutcome, NotRun>> + 'a {
    handler.handle_block(cid, &[], BlockMetadata::recovery())
}

#[test]
fn a_saturated_admission_times_out_and_a_dropped_wait_frees_its_place() {
    let pool = workers::Pool::start(workers::Config::unplaced(2)).unwrap();
    let inner = Arc::new(Held::default());
    let cid = Cid::default();
    let handler = PooledMergeHandler::with_admission(
        Arc::clone(&inner),
        Some(pool.clone()),
        1,
        Duration::from_millis(200),
    );

    let (held, waited) = workers::block_on_timeout(
        async {
            futures::join!(merge(&handler, &cid), async {
                let waited = merge(&handler, &cid).await;
                {
                    let mut dropped = pin!(merge(&handler, &cid));
                    let waker = futures::task::noop_waker();
                    assert!(dropped
                        .as_mut()
                        .poll(&mut Context::from_waker(&waker))
                        .is_pending());
                }
                inner.release.set();
                waited
            })
        },
        Duration::from_secs(10),
    )
    .unwrap();

    assert_eq!(held.unwrap(), MergeOutcome::Merged);
    assert!(matches!(waited, Err(NotRun(workers::Error::TimedOut))));
    assert_eq!(handler.admission.available_permits(), 1);
    let after = workers::block_on_timeout(merge(&handler, &cid), Duration::from_secs(10)).unwrap();
    assert_eq!(after.unwrap(), MergeOutcome::Merged);
    pool.shutdown_blocking().unwrap();
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
fn a_batch_behind_a_saturated_admission_times_out_every_block_and_applies_none() {
    let pool = workers::Pool::start(workers::Config::unplaced(2)).unwrap();
    let inner = Arc::new(Held::default());
    let cid = Cid::default();
    let handler = PooledMergeHandler::with_admission(
        Arc::clone(&inner),
        Some(pool.clone()),
        1,
        Duration::from_millis(200),
    );
    let blocks = [block("bae-1"), block("bae-2"), block("bae-1")];

    let (held, batched) = workers::block_on_timeout(
        async {
            futures::join!(merge(&handler, &cid), async {
                let batched = handler.handle_block_batch(&blocks).await;
                let merged_while_held = inner.merged.load(Ordering::SeqCst);
                inner.release.set();
                (batched, merged_while_held)
            })
        },
        Duration::from_secs(10),
    )
    .unwrap();

    assert_eq!(held.unwrap(), MergeOutcome::Merged);
    let (batched, merged_while_held) = batched;
    assert_eq!(merged_while_held, 0);
    assert_eq!(batched.len(), blocks.len());
    for outcome in batched {
        assert!(matches!(outcome, Err(NotRun(workers::Error::TimedOut))));
    }
    assert_eq!(inner.merged.load(Ordering::SeqCst), 1);
    assert_eq!(handler.admission.available_permits(), 1);
    pool.shutdown_blocking().unwrap();
}

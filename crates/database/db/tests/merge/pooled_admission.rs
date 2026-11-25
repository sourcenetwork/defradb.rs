//! A merge waits for an admission slot no longer than its deadline, and a
//! wait that is dropped leaves no slot taken.

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::Context;
use std::time::Duration;

use async_trait::async_trait;
use cid::Cid;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
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

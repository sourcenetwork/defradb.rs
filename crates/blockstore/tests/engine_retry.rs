//! A block write that loses a race to another writer of the same block is
//! re-run by the engine and settles: the second put finds the block stored
//! and leaves it, so no conflict reaches the caller.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use blockstore::{Blockstore, DefraBlockstore};
use cid::Cid;
use storage::corekv::{
    Key, Result, RetryAttempt, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture,
    RetryAttemptValue, RetryOptions, RetryTxn, Store, Txn,
};
use storage::keys::blockstore::{BlockstoreKey, ToMergeIndexKey};
use storage::namespace::{Namespace, NamespacedStore};
use storage::RegolithStore;

fn cid() -> Cid {
    Cid::from_str("bafkreigh2akiscaildcqabsyg3dfr6chu3fgpregiymsck7e7aqa4s52zy").unwrap()
}

/// Once, after the next engine attempt has read the block as absent and
/// written it with its unmerged mark, another path stores and merges the same
/// block, so the attempt's mark loses to the newer delete at commit.
struct RacingStore {
    inner: Arc<RegolithStore>,
    armed: Arc<AtomicBool>,
    attempts: Arc<AtomicUsize>,
}

/// Another path stores the block and merges it: the block is written and its
/// unmerged mark removed.
async fn competing_store_and_merge(store: &Arc<RegolithStore>) -> Result<()> {
    let blocks = NamespacedStore::new(Arc::clone(store), Namespace::Blockstore);
    let mut txn = blocks.new_txn(false).await?;
    txn.set(&BlockstoreKey::new(cid()).bytes(), b"block")
        .await?;
    txn.delete(&ToMergeIndexKey::new(cid()).bytes()).await?;
    txn.commit().await
}

impl storage::corekv::private::Sealed for RacingStore {}

#[async_trait]
impl Store for RacingStore {
    async fn allocate(&self, key: &[u8], count: u64) -> Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: RetryOptions,
        attempt: RetryAttempt,
    ) -> std::result::Result<RetryAttemptValue, RetryAttemptError> {
        let attempt = Box::new(RacedAttempt {
            inner: attempt,
            store: Arc::clone(&self.inner),
            armed: Arc::clone(&self.armed),
            attempts: Arc::clone(&self.attempts),
        });
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}

struct RacedAttempt {
    inner: RetryAttempt,
    store: Arc<RegolithStore>,
    armed: Arc<AtomicBool>,
    attempts: Arc<AtomicUsize>,
}

impl RetryAttemptFn for RacedAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            let output = self.inner.run(txn).await?;
            if self.armed.swap(false, Ordering::SeqCst) {
                competing_store_and_merge(&self.store).await?;
            }
            Ok(output)
        })
    }
}

fn racing_store() -> Arc<RacingStore> {
    Arc::new(RacingStore {
        inner: Arc::new(RegolithStore::in_memory().unwrap()),
        armed: Arc::new(AtomicBool::new(true)),
        attempts: Arc::new(AtomicUsize::new(0)),
    })
}

#[test]
fn a_put_that_loses_to_the_same_block_settles_on_the_re_run() {
    workers::block_on(async {
        let store = racing_store();
        let blockstore = DefraBlockstore::new(store.clone(), true);

        blockstore.put(&cid(), b"block").await.unwrap();

        assert_eq!(store.attempts.load(Ordering::SeqCst), 2);
        assert_eq!(
            blockstore.get(&cid()).await.unwrap().as_deref(),
            Some(b"block".as_slice())
        );
        // The re-run found the block stored and left it, merged mark and all.
        assert!(blockstore.is_merged(&cid()).await.unwrap());
    })
    .unwrap()
}

#[test]
fn a_put_many_that_loses_to_one_of_its_blocks_settles_on_the_re_run() {
    workers::block_on(async {
        let store = racing_store();
        let blockstore = DefraBlockstore::new(store.clone(), true);
        let other =
            Cid::from_str("bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku").unwrap();

        blockstore
            .put_many(&[(&cid(), b"block".as_slice()), (&other, b"other".as_slice())])
            .await
            .unwrap();

        assert_eq!(store.attempts.load(Ordering::SeqCst), 2);
        assert!(blockstore.has(&cid()).await.unwrap());
        assert_eq!(
            blockstore.get(&other).await.unwrap().as_deref(),
            Some(b"other".as_slice())
        );
    })
    .unwrap()
}

#[test]
fn block_writes_run_under_the_configured_retry_budget() {
    workers::block_on(async {
        let store = racing_store();
        let blockstore = DefraBlockstore::new(store.clone(), true).with_max_txn_retries(0);

        let error = blockstore.put(&cid(), b"block").await.unwrap_err();

        assert!(
            matches!(
                error,
                blockstore::Error::Storage(storage::corekv::Error::TxnRetryExhausted(_))
            ),
            "{error:?}"
        );
        assert_eq!(store.attempts.load(Ordering::SeqCst), 1);
    })
    .unwrap()
}

/// Many writers store one new block at once. Each conflict is settled by the
/// engine: the losers' re-runs find the block stored, so every put succeeds.
#[test]
fn concurrent_puts_of_one_new_block_all_succeed() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        for round in 0..64_u32 {
            let data = round.to_be_bytes().to_vec();
            let block = {
                use multihash_codetable::MultihashDigest;
                Cid::new_v1(0x55, multihash_codetable::Code::Sha2_256.digest(&data))
            };
            let writers: Vec<_> = (0..16)
                .map(|_| {
                    let blockstore = Arc::clone(&blockstore);
                    let data = data.clone();
                    workers::spawn(None, "test", move || async move {
                        blockstore.put(&block, &data).await
                    })
                    .unwrap()
                })
                .collect();
            for writer in writers {
                writer.await.unwrap().unwrap();
            }
            assert_eq!(
                blockstore.get(&block).await.unwrap().as_deref(),
                Some(data.as_slice())
            );
        }
    })
    .unwrap()
}

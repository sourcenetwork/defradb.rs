//! A blockstore placed on a pool makes its reads and writes on a worker when
//! called off the pool, which the store counts.

use std::sync::Arc;

use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};
use storage::corekv::Store;

use crate::{Blockstore, DefraBlockstore};

#[test]
fn a_placed_blockstore_reads_and_writes_on_the_pool() {
    let pool = workers::Pool::start(storage::with_worker_io(workers::Config::unplaced(1))).unwrap();
    let options = storage::RegolithStoreOptions::memory().placed_on(&pool);
    let store =
        Arc::new(storage::RegolithStore::open_with_options("regolith-memory", options).unwrap());
    let stats = store.transaction_stats_handle().unwrap();
    let blocks = DefraBlockstore::new(Arc::clone(&store), true).placed_on(Some(&pool));
    let data = b"block".as_slice();
    let cid = Cid::new_v1(0x71, Code::Sha2_256.digest(data));
    let before = stats.snapshot().off_pool_calls;

    workers::block_on(async {
        blocks.put(&cid, data).await.unwrap();
        assert_eq!(blocks.get(&cid).await.unwrap().as_deref(), Some(data));
        assert!(blocks.has(&cid).await.unwrap());
        assert!(!blocks.is_merged(&cid).await.unwrap());
        blocks.mark_as_merged(&cid).await.unwrap();
        assert!(blocks.is_merged(&cid).await.unwrap());
        assert_eq!(blocks.all_cids().await.unwrap(), vec![cid]);
    })
    .unwrap();

    assert_eq!(
        stats.snapshot().off_pool_calls,
        before,
        "a block read or write ran off the pool"
    );
    pool.shutdown_blocking().unwrap();
}

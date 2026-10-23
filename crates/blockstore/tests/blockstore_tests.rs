//! Integration tests for the blockstore crate
//!
//! Organized into submodules:
//! - `basic_crud` — Put, get, has, delete, size, all_cids, deduplication
//! - `hash_verification` — hash_on_read toggle, valid/corrupted/unsupported hashes
//! - `merge_tracking` — P2P merge lifecycle, local mode, filtering
//! - `go_compat` — Go behavior parity (CIDv0, immutability, key format)
//! - `concurrency` — Concurrent reads, writes, deletes, merge ops
//! - `stress` — Many-block and batch scaling
//! - `error_paths` — Malformed keys, key format invariants

use std::str::FromStr;
use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore, Error};
use bytes::Bytes;
use cid::Cid;
use storage::corekv::{Key, Store};
use storage::stores::blockstore::BlockstoreTxn;
use storage::RegolithStore;

fn test_cid() -> Cid {
    Cid::from_str("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap()
}

fn test_cid2() -> Cid {
    Cid::from_str("bafkreigh2akiscaildcqabsyg3dfr6chu3fgpregiymsck7e7aqa4s52zy").unwrap()
}

fn test_cid3() -> Cid {
    Cid::from_str("bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku").unwrap()
}

/// Create a CID from data using SHA2-256 (for hash verification tests)
fn cid_from_data(data: &[u8]) -> Cid {
    use multihash::Multihash;
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();

    let hash = Multihash::<64>::wrap(0x12, &digest).unwrap();
    Cid::new_v1(0x55, hash) // raw codec
}

mod basic_crud {
    use super::*;

    #[test]
    fn put_get() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid = test_cid();
            let data = b"hello world";

            blockstore.put(&cid, data).await.unwrap();

            let retrieved = blockstore.get(&cid).await.unwrap();
            assert_eq!(retrieved, Some(Bytes::copy_from_slice(data)));
        })
        .unwrap()
    }

    #[test]
    fn get_nonexistent_block() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let result = blockstore.get(&test_cid()).await.unwrap();
            assert_eq!(result, None);
        })
        .unwrap()
    }

    #[test]
    fn has() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid = test_cid();
            let data = b"test data";

            assert!(!blockstore.has(&cid).await.unwrap());
            blockstore.put(&cid, data).await.unwrap();
            assert!(blockstore.has(&cid).await.unwrap());
        })
        .unwrap()
    }

    #[test]
    fn delete() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid = test_cid();
            let data = b"to be deleted";

            blockstore.put(&cid, data).await.unwrap();
            assert!(blockstore.has(&cid).await.unwrap());

            blockstore.delete(&cid).await.unwrap();

            assert!(!blockstore.has(&cid).await.unwrap());
            assert_eq!(blockstore.get(&cid).await.unwrap(), None);
        })
        .unwrap()
    }

    #[test]
    fn get_size() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid = test_cid();
            let data = b"test data for size";

            assert_eq!(blockstore.get_size(&cid).await.unwrap(), None);

            blockstore.put(&cid, data).await.unwrap();

            let size = blockstore.get_size(&cid).await.unwrap();
            assert_eq!(size, Some(data.len()));
        })
        .unwrap()
    }

    #[test]
    fn put_many() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid1 = test_cid();
            let cid2 = test_cid2();
            let data1 = b"block one";
            let data2 = b"block two";

            let blocks: Vec<(&Cid, &[u8])> =
                vec![(&cid1, data1.as_slice()), (&cid2, data2.as_slice())];
            blockstore.put_many(&blocks).await.unwrap();

            assert_eq!(
                blockstore.get(&cid1).await.unwrap(),
                Some(Bytes::copy_from_slice(data1))
            );
            assert_eq!(
                blockstore.get(&cid2).await.unwrap(),
                Some(Bytes::copy_from_slice(data2))
            );
        })
        .unwrap()
    }

    #[test]
    fn put_many_empty() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let blocks: Vec<(&Cid, &[u8])> = vec![];
            blockstore.put_many(&blocks).await.unwrap();
        })
        .unwrap()
    }

    #[test]
    fn all_cids() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid1 = test_cid();
            let cid2 = test_cid2();
            let cid3 = test_cid3();

            let cids = blockstore.all_cids().await.unwrap();
            assert!(cids.is_empty());

            blockstore.put(&cid1, b"data1").await.unwrap();
            blockstore.put(&cid2, b"data2").await.unwrap();
            blockstore.put(&cid3, b"data3").await.unwrap();

            let cids = blockstore.all_cids().await.unwrap();
            assert_eq!(cids.len(), 3);
            assert!(cids.contains(&cid1));
            assert!(cids.contains(&cid2));
            assert!(cids.contains(&cid3));
        })
        .unwrap()
    }

    #[test]
    fn deduplication() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid = test_cid();
            let data = b"original data";

            blockstore.put(&cid, data).await.unwrap();
            blockstore.put(&cid, data).await.unwrap();

            let cids = blockstore.all_cids().await.unwrap();
            assert_eq!(cids.len(), 1);
            assert_eq!(cids[0], cid);
        })
        .unwrap()
    }
}

mod hash_verification {
    use super::*;

    #[test]
    fn disabled_by_default() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);
            assert!(!blockstore.rehash_enabled());
        })
        .unwrap()
    }

    #[test]
    fn enable_disable() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            blockstore.hash_on_read(true);
            assert!(blockstore.rehash_enabled());

            blockstore.hash_on_read(false);
            assert!(!blockstore.rehash_enabled());
        })
        .unwrap()
    }

    #[test]
    fn valid_data() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let data = b"test data for hash verification";
            let cid = cid_from_data(data);

            blockstore.put(&cid, data).await.unwrap();
            blockstore.hash_on_read(true);

            let result = blockstore.get(&cid).await.unwrap();
            assert_eq!(result, Some(Bytes::copy_from_slice(data)));
        })
        .unwrap()
    }

    #[test]
    fn corrupted_data() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let original_data = b"original data";
            let corrupted_data = b"corrupted data";
            let cid = cid_from_data(original_data);

            {
                let mut txn = blockstore.new_store_txn(false).await.unwrap();
                let bs_txn = txn.as_any_mut().downcast_mut::<BlockstoreTxn>().unwrap();
                bs_txn.put_block(&cid, corrupted_data).await.unwrap();
                txn.commit().await.unwrap();
            }

            blockstore.hash_on_read(false);
            let result = blockstore.get(&cid).await.unwrap();
            assert_eq!(result, Some(Bytes::from(corrupted_data.to_vec())));

            blockstore.hash_on_read(true);
            let result = blockstore.get(&cid).await;
            assert!(result.is_err());
            match result {
                Err(Error::HashMismatch { cid: cid_str }) => {
                    assert_eq!(cid_str, cid.to_string());
                }
                other => panic!("Expected HashMismatch error, got {:?}", other),
            }
        })
        .unwrap()
    }

    #[test]
    fn nonexistent_block() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            blockstore.hash_on_read(true);

            let result = blockstore.get(&test_cid()).await.unwrap();
            assert_eq!(result, None);
        })
        .unwrap()
    }

    #[test]
    fn unsupported_algorithm_skipped() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            use multihash::Multihash;
            let data = b"identity hash data";
            let hash = Multihash::<64>::wrap(0x00, data).unwrap();
            let cid = Cid::new_v1(0x55, hash);

            blockstore.put(&cid, data).await.unwrap();
            blockstore.hash_on_read(true);

            let result = blockstore.get(&cid).await.unwrap();
            assert_eq!(result, Some(Bytes::copy_from_slice(data)));
        })
        .unwrap()
    }

    #[test]
    fn blake2b_skipped() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            use multihash::Multihash;
            let data = b"blake2b test data";
            let fake_digest = [0u8; 32];
            let hash = Multihash::<64>::wrap(0xb220, &fake_digest).unwrap();
            let cid = Cid::new_v1(0x55, hash);

            blockstore.put(&cid, data).await.unwrap();
            blockstore.hash_on_read(true);

            let result = blockstore.get(&cid).await.unwrap();
            assert_eq!(result, Some(Bytes::copy_from_slice(data)));
        })
        .unwrap()
    }

    #[test]
    fn empty_block() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let data: &[u8] = b"";
            let cid = cid_from_data(data);

            blockstore.put(&cid, data).await.unwrap();

            let retrieved = blockstore.get(&cid).await.unwrap();
            assert_eq!(retrieved, Some(Bytes::new()));
            assert_eq!(blockstore.get_size(&cid).await.unwrap(), Some(0));
            assert!(blockstore.has(&cid).await.unwrap());

            blockstore.hash_on_read(true);
            let verified = blockstore.get(&cid).await.unwrap();
            assert_eq!(verified, Some(Bytes::new()));
        })
        .unwrap()
    }

    #[test]
    fn large_block() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let data: Vec<u8> = (0..262144).map(|i| (i % 256) as u8).collect();
            let cid = cid_from_data(&data);

            blockstore.put(&cid, &data).await.unwrap();

            let retrieved = blockstore.get(&cid).await.unwrap();
            assert_eq!(retrieved, Some(Bytes::from(data.clone())));
            assert_eq!(blockstore.get_size(&cid).await.unwrap(), Some(262144));

            blockstore.hash_on_read(true);
            let verified = blockstore.get(&cid).await.unwrap();
            assert_eq!(verified, Some(Bytes::from(data)));
        })
        .unwrap()
    }

    #[test]
    fn large_block_many() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let data1: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
            let data2: Vec<u8> = (0..65536).map(|i| ((i + 100) % 256) as u8).collect();
            let data3: Vec<u8> = (0..65536).map(|i| ((i + 200) % 256) as u8).collect();

            let cid1 = cid_from_data(&data1);
            let cid2 = cid_from_data(&data2);
            let cid3 = cid_from_data(&data3);

            let blocks: Vec<(&Cid, &[u8])> = vec![
                (&cid1, data1.as_slice()),
                (&cid2, data2.as_slice()),
                (&cid3, data3.as_slice()),
            ];
            blockstore.put_many(&blocks).await.unwrap();

            assert_eq!(
                blockstore.get(&cid1).await.unwrap(),
                Some(Bytes::from(data1))
            );
            assert_eq!(
                blockstore.get(&cid2).await.unwrap(),
                Some(Bytes::from(data2))
            );
            assert_eq!(
                blockstore.get(&cid3).await.unwrap(),
                Some(Bytes::from(data3))
            );
        })
        .unwrap()
    }
}

mod merge_tracking {
    use super::*;

    #[test]
    fn p2p_lifecycle() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid = test_cid();
            let data = b"p2p block";

            blockstore.put(&cid, data).await.unwrap();
            assert!(!blockstore.is_merged(&cid).await.unwrap());

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(unmerged.contains(&cid));

            blockstore.mark_as_merged(&cid).await.unwrap();
            assert!(blockstore.is_merged(&cid).await.unwrap());

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(!unmerged.contains(&cid));
        })
        .unwrap()
    }

    #[test]
    fn local_mode_no_tracking() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid = test_cid();
            blockstore.put(&cid, b"local block").await.unwrap();

            assert!(blockstore.is_merged(&cid).await.unwrap());

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(unmerged.is_empty());
        })
        .unwrap()
    }

    #[test]
    fn is_merged_nonexistent_p2p() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);
            assert!(!blockstore.is_merged(&test_cid()).await.unwrap());
        })
        .unwrap()
    }

    #[test]
    fn is_merged_nonexistent_local() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);
            assert!(!blockstore.is_merged(&test_cid()).await.unwrap());
        })
        .unwrap()
    }

    #[test]
    fn unmerged_filtering() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid1 = test_cid();
            let cid2 = test_cid2();

            blockstore.put(&cid1, b"data1").await.unwrap();
            blockstore.put(&cid2, b"data2").await.unwrap();

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert_eq!(unmerged.len(), 2);

            blockstore.mark_as_merged(&cid1).await.unwrap();

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert_eq!(unmerged.len(), 1);
            assert!(unmerged.contains(&cid2));
            assert!(!unmerged.contains(&cid1));
        })
        .unwrap()
    }

    #[test]
    fn all_cids_excludes_merge_markers() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid1 = test_cid();
            let cid2 = test_cid2();

            blockstore.put(&cid1, b"data1").await.unwrap();
            blockstore.put(&cid2, b"data2").await.unwrap();

            let cids = blockstore.all_cids().await.unwrap();
            assert_eq!(cids.len(), 2);
            assert!(cids.contains(&cid1));
            assert!(cids.contains(&cid2));
        })
        .unwrap()
    }

    #[test]
    fn delete_removes_merge_marker() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid = test_cid();
            blockstore.put(&cid, b"data").await.unwrap();
            assert!(!blockstore.is_merged(&cid).await.unwrap());

            blockstore.delete(&cid).await.unwrap();

            assert!(!blockstore.has(&cid).await.unwrap());
            assert!(!blockstore.is_merged(&cid).await.unwrap());

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(!unmerged.contains(&cid));
        })
        .unwrap()
    }

    #[test]
    fn mark_nonexistent_is_noop() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid = test_cid();
            let result = blockstore.mark_as_merged(&cid).await;
            assert!(result.is_ok());
            assert!(!blockstore.has(&cid).await.unwrap());
            assert!(!blockstore.is_merged(&cid).await.unwrap());
        })
        .unwrap()
    }

    #[test]
    fn delete_then_is_merged() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid = test_cid();
            blockstore
                .put(&cid, b"block to merge then delete")
                .await
                .unwrap();
            blockstore.mark_as_merged(&cid).await.unwrap();
            assert!(blockstore.is_merged(&cid).await.unwrap());

            blockstore.delete(&cid).await.unwrap();

            assert!(
                !blockstore.is_merged(&cid).await.unwrap(),
                "is_merged should return false for deleted block"
            );
            assert!(!blockstore.has(&cid).await.unwrap());
            assert_eq!(blockstore.get(&cid).await.unwrap(), None);

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(!unmerged.contains(&cid));
        })
        .unwrap()
    }

    #[test]
    fn delete_unmerged_block() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid = test_cid();
            blockstore.put(&cid, b"unmerged block").await.unwrap();
            assert!(!blockstore.is_merged(&cid).await.unwrap());

            blockstore.delete(&cid).await.unwrap();

            assert!(!blockstore.is_merged(&cid).await.unwrap());

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(!unmerged.contains(&cid));
        })
        .unwrap()
    }

    #[test]
    fn merge_delete_reput() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid = test_cid();

            blockstore.put(&cid, b"first").await.unwrap();
            blockstore.mark_as_merged(&cid).await.unwrap();
            assert!(blockstore.is_merged(&cid).await.unwrap());

            blockstore.delete(&cid).await.unwrap();
            assert!(!blockstore.is_merged(&cid).await.unwrap());

            blockstore.put(&cid, b"second").await.unwrap();

            assert!(
                !blockstore.is_merged(&cid).await.unwrap(),
                "Re-added block should be unmerged"
            );

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(
                unmerged.contains(&cid),
                "Re-added block should appear in unmerged list"
            );
        })
        .unwrap()
    }
}

mod go_compat {
    use super::*;

    #[test]
    fn get_with_default_cid_returns_none() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let zero_bytes = vec![0x12, 0x20];
            let result = Cid::try_from(zero_bytes.as_slice());
            assert!(result.is_err());

            let empty_data_cid = cid_from_data(b"");
            let result = blockstore.get(&empty_data_cid).await.unwrap();
            assert_eq!(result, None);
        })
        .unwrap()
    }

    #[test]
    fn operations_with_cidv0() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cidv0 = Cid::from_str("QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n").unwrap();
            let data = b"cidv0 test data";

            blockstore.put(&cidv0, data).await.unwrap();
            assert!(blockstore.has(&cidv0).await.unwrap());
            assert_eq!(
                blockstore.get(&cidv0).await.unwrap(),
                Some(Bytes::copy_from_slice(data))
            );
            assert_eq!(blockstore.get_size(&cidv0).await.unwrap(), Some(data.len()));

            let cids = blockstore.all_cids().await.unwrap();
            assert!(cids.contains(&cidv0));

            blockstore.delete(&cidv0).await.unwrap();
            assert!(!blockstore.has(&cidv0).await.unwrap());
        })
        .unwrap()
    }

    #[test]
    fn put_already_merged_stays_merged() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid = test_cid();
            let data = b"original data";

            blockstore.put(&cid, data).await.unwrap();
            assert!(!blockstore.is_merged(&cid).await.unwrap());

            blockstore.mark_as_merged(&cid).await.unwrap();
            assert!(blockstore.is_merged(&cid).await.unwrap());

            blockstore.put(&cid, data).await.unwrap();

            assert!(
                blockstore.is_merged(&cid).await.unwrap(),
                "Re-putting an already-merged block should not create a new merge marker"
            );

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(
                !unmerged.contains(&cid),
                "Re-put block should not appear in unmerged list"
            );
        })
        .unwrap()
    }

    #[test]
    fn put_many_with_existing_merged() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid1 = test_cid();
            let cid2 = test_cid2();
            let data1 = b"data one";
            let data2 = b"data two";

            blockstore.put(&cid1, data1).await.unwrap();
            blockstore.mark_as_merged(&cid1).await.unwrap();
            assert!(blockstore.is_merged(&cid1).await.unwrap());

            let blocks: Vec<(&Cid, &[u8])> =
                vec![(&cid1, data1.as_slice()), (&cid2, data2.as_slice())];
            blockstore.put_many(&blocks).await.unwrap();

            assert!(
                blockstore.is_merged(&cid1).await.unwrap(),
                "Existing merged block should stay merged after put_many"
            );
            assert!(!blockstore.is_merged(&cid2).await.unwrap());

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(!unmerged.contains(&cid1));
            assert!(unmerged.contains(&cid2));
        })
        .unwrap()
    }

    #[test]
    fn same_cid_different_data_no_overwrite() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid = test_cid();
            let original_data = b"original data";
            let new_data = b"different data";

            blockstore.put(&cid, original_data).await.unwrap();
            blockstore.put(&cid, new_data).await.unwrap();

            let retrieved = blockstore.get(&cid).await.unwrap();
            assert_eq!(retrieved, Some(Bytes::copy_from_slice(original_data)));
        })
        .unwrap()
    }

    #[test]
    fn put_many_duplicate_cids_in_batch() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid = test_cid();
            let data1 = b"first data";
            let data2 = b"second data";

            let blocks: Vec<(&Cid, &[u8])> =
                vec![(&cid, data1.as_slice()), (&cid, data2.as_slice())];
            blockstore.put_many(&blocks).await.unwrap();

            let retrieved = blockstore.get(&cid).await.unwrap();
            assert_eq!(retrieved, Some(Bytes::copy_from_slice(data1)));

            let cids = blockstore.all_cids().await.unwrap();
            assert_eq!(cids.len(), 1);
        })
        .unwrap()
    }

    #[test]
    fn delete_nonexistent_is_noop() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            let cid = test_cid();
            let result = blockstore.delete(&cid).await;
            assert!(result.is_ok());
            assert!(!blockstore.has(&cid).await.unwrap());
        })
        .unwrap()
    }

    #[test]
    fn delete_nonexistent_p2p() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            let cid = test_cid();
            let result = blockstore.delete(&cid).await;
            assert!(result.is_ok());

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(!unmerged.contains(&cid));
        })
        .unwrap()
    }

    #[test]
    fn key_format_compatibility() {
        workers::block_on(async {
            use storage::keys::blockstore::{BlockstoreKey, ToMergeIndexKey, MERGE_PREFIX};

            let cid = test_cid();

            let block_key = BlockstoreKey::new(cid);
            let block_bytes = block_key.bytes();
            assert_eq!(
                block_bytes,
                cid.to_bytes(),
                "Block key should be raw CID bytes"
            );

            let merge_key = ToMergeIndexKey::new(cid);
            let merge_bytes = merge_key.bytes();

            assert_eq!(merge_bytes[0], MERGE_PREFIX);
            assert_eq!(merge_bytes[0], b'm');
            assert_eq!(merge_bytes[0], 0x6D);
            assert_eq!(&merge_bytes[1..], cid.to_bytes().as_slice());
            assert_eq!(merge_bytes.len(), 1 + cid.to_bytes().len());

            let cidv0 = Cid::from_str("QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n").unwrap();
            let merge_key_v0 = ToMergeIndexKey::new(cidv0);
            let merge_bytes_v0 = merge_key_v0.bytes();
            assert_eq!(merge_bytes_v0[0], b'm');
            assert_eq!(&merge_bytes_v0[1..], cidv0.to_bytes().as_slice());
        })
        .unwrap()
    }
}

mod concurrency {
    use super::*;

    #[test]
    fn put_different_cids() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = Arc::new(DefraBlockstore::new(store, false));

            let cid1 = test_cid();
            let cid2 = test_cid2();
            let cid3 = test_cid3();
            let data1 = b"concurrent data 1";
            let data2 = b"concurrent data 2";
            let data3 = b"concurrent data 3";

            let bs1 = blockstore.clone();
            let bs2 = blockstore.clone();
            let bs3 = blockstore.clone();

            let (r1, r2, r3) = futures::join!(
                async move { bs1.put(&cid1, data1).await },
                async move { bs2.put(&cid2, data2).await },
                async move { bs3.put(&cid3, data3).await }
            );

            r1.unwrap();
            r2.unwrap();
            r3.unwrap();

            assert_eq!(
                blockstore.get(&cid1).await.unwrap(),
                Some(Bytes::copy_from_slice(data1))
            );
            assert_eq!(
                blockstore.get(&cid2).await.unwrap(),
                Some(Bytes::copy_from_slice(data2))
            );
            assert_eq!(
                blockstore.get(&cid3).await.unwrap(),
                Some(Bytes::copy_from_slice(data3))
            );
        })
        .unwrap()
    }

    #[test]
    fn put_same_cid() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = Arc::new(DefraBlockstore::new(store, false));

            let cid = test_cid();
            let data1 = b"first writer";
            let data2 = b"second writer";

            let bs1 = blockstore.clone();
            let bs2 = blockstore.clone();

            let (r1, r2) = futures::join!(async move { bs1.put(&cid, data1).await }, async move {
                bs2.put(&cid, data2).await
            });

            r1.unwrap();
            r2.unwrap();

            let retrieved = blockstore.get(&cid).await.unwrap();
            assert!(
                retrieved == Some(Bytes::copy_from_slice(data1))
                    || retrieved == Some(Bytes::copy_from_slice(data2))
            );

            let cids = blockstore.all_cids().await.unwrap();
            assert_eq!(cids.len(), 1);
        })
        .unwrap()
    }

    #[test]
    fn get_and_put() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = Arc::new(DefraBlockstore::new(store, false));

            let cid = test_cid();
            let data = b"concurrent access data";

            blockstore.put(&cid, data).await.unwrap();

            let bs1 = blockstore.clone();
            let bs2 = blockstore.clone();
            let bs3 = blockstore.clone();

            let cid2 = test_cid2();
            let (r1, r2, r3) = futures::join!(
                async move { bs1.get(&cid).await },
                async move { bs2.get(&cid).await },
                async move { bs3.put(&cid2, b"other data").await }
            );

            assert_eq!(r1.unwrap(), Some(Bytes::copy_from_slice(data)));
            assert_eq!(r2.unwrap(), Some(Bytes::copy_from_slice(data)));
            r3.unwrap();
        })
        .unwrap()
    }

    #[test]
    fn hash_on_read_toggle() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = Arc::new(DefraBlockstore::new(store, false));

            let data = b"hash verification data";
            let cid = cid_from_data(data);
            blockstore.put(&cid, data).await.unwrap();

            let bs1 = blockstore.clone();
            let bs2 = blockstore.clone();
            let bs3 = blockstore.clone();

            let (_, _, r3) = futures::join!(
                async move {
                    bs1.hash_on_read(true);
                },
                async move {
                    bs2.hash_on_read(false);
                },
                async move { bs3.get(&cid).await }
            );

            assert_eq!(r3.unwrap(), Some(Bytes::copy_from_slice(data)));
        })
        .unwrap()
    }

    #[test]
    fn p2p_merge_tracking() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = Arc::new(DefraBlockstore::new(store, true));

            let cid1 = test_cid();
            let cid2 = test_cid2();

            blockstore.put(&cid1, b"block1").await.unwrap();
            blockstore.put(&cid2, b"block2").await.unwrap();

            let bs1 = blockstore.clone();
            let bs2 = blockstore.clone();

            let (r1, r2) =
                futures::join!(async move { bs1.mark_as_merged(&cid1).await }, async move {
                    bs2.mark_as_merged(&cid2).await
                });

            r1.unwrap();
            r2.unwrap();

            assert!(blockstore.is_merged(&cid1).await.unwrap());
            assert!(blockstore.is_merged(&cid2).await.unwrap());

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert!(unmerged.is_empty());
        })
        .unwrap()
    }

    #[test]
    fn delete_during_read() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = Arc::new(DefraBlockstore::new(store, false));

            let cid = test_cid();
            let data = b"data that may be deleted";

            blockstore.put(&cid, data).await.unwrap();

            for _ in 0..10 {
                if !blockstore.has(&cid).await.unwrap() {
                    blockstore.put(&cid, data).await.unwrap();
                }

                let bs_read = blockstore.clone();
                let bs_delete = blockstore.clone();

                let (read_result, delete_result) =
                    futures::join!(async move { bs_read.get(&cid).await }, async move {
                        bs_delete.delete(&cid).await
                    });

                assert!(delete_result.is_ok());

                let read_value = read_result.unwrap();
                assert!(
                    read_value.is_none() || read_value == Some(Bytes::copy_from_slice(data)),
                    "Read during delete should return None or valid data, got {:?}",
                    read_value
                );
            }
        })
        .unwrap()
    }

    #[test]
    fn delete_and_has() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = Arc::new(DefraBlockstore::new(store, false));

            let cid = test_cid();
            blockstore.put(&cid, b"data").await.unwrap();

            for _ in 0..10 {
                if !blockstore.has(&cid).await.unwrap() {
                    blockstore.put(&cid, b"data").await.unwrap();
                }

                let bs_has = blockstore.clone();
                let bs_delete = blockstore.clone();

                let (has_result, delete_result) =
                    futures::join!(async move { bs_has.has(&cid).await }, async move {
                        bs_delete.delete(&cid).await
                    });

                assert!(delete_result.is_ok());
                assert!(has_result.is_ok());
            }
        })
        .unwrap()
    }
}

mod stress {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn many_blocks() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            const NUM_BLOCKS: usize = 500;
            let mut cids = Vec::with_capacity(NUM_BLOCKS);

            for i in 0..NUM_BLOCKS {
                let data = format!("block data {}", i);
                let cid = cid_from_data(data.as_bytes());
                blockstore.put(&cid, data.as_bytes()).await.unwrap();
                cids.push(cid);
            }

            let all = blockstore.all_cids().await.unwrap();
            assert_eq!(
                all.len(),
                NUM_BLOCKS,
                "all_cids should return all {} blocks",
                NUM_BLOCKS
            );

            for cid in &cids {
                assert!(all.contains(cid), "Missing CID: {}", cid);
            }

            for (i, cid) in cids.iter().enumerate().step_by(50) {
                let expected = format!("block data {}", i);
                let data = blockstore.get(cid).await.unwrap();
                assert_eq!(data, Some(Bytes::from(expected.into_bytes())));
            }

            for cid in &cids {
                assert!(blockstore.has(cid).await.unwrap());
            }
        })
        .unwrap()
    }

    #[test]
    fn many_blocks_p2p_merge() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, true);

            const NUM_BLOCKS: usize = 200;
            let mut cids = Vec::with_capacity(NUM_BLOCKS);

            for i in 0..NUM_BLOCKS {
                let data = format!("p2p block {}", i);
                let cid = cid_from_data(data.as_bytes());
                blockstore.put(&cid, data.as_bytes()).await.unwrap();
                cids.push(cid);
            }

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert_eq!(unmerged.len(), NUM_BLOCKS);

            for cid in cids.iter().take(NUM_BLOCKS / 2) {
                blockstore.mark_as_merged(cid).await.unwrap();
            }

            let unmerged = blockstore.get_unmerged().await.unwrap();
            assert_eq!(
                unmerged.len(),
                NUM_BLOCKS / 2,
                "Half should still be unmerged"
            );

            for (i, cid) in cids.iter().enumerate() {
                let is_merged = blockstore.is_merged(cid).await.unwrap();
                if i < NUM_BLOCKS / 2 {
                    assert!(is_merged, "Block {} should be merged", i);
                } else {
                    assert!(!is_merged, "Block {} should be unmerged", i);
                }
            }

            let all = blockstore.all_cids().await.unwrap();
            assert_eq!(all.len(), NUM_BLOCKS);
        })
        .unwrap()
    }

    #[test]
    fn put_many_batch() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);

            const BATCH_SIZE: usize = 100;

            let blocks: Vec<(Cid, Vec<u8>)> = (0..BATCH_SIZE)
                .map(|i| {
                    let data = format!("batch block {}", i).into_bytes();
                    let cid = cid_from_data(&data);
                    (cid, data)
                })
                .collect();

            let block_refs: Vec<(&Cid, &[u8])> =
                blocks.iter().map(|(c, d)| (c, d.as_slice())).collect();

            blockstore.put_many(&block_refs).await.unwrap();

            for (cid, expected_data) in &blocks {
                let data = blockstore.get(cid).await.unwrap();
                assert_eq!(data, Some(Bytes::from(expected_data.clone())));
            }

            let all = blockstore.all_cids().await.unwrap();
            assert_eq!(all.len(), BATCH_SIZE);
        })
        .unwrap()
    }

    #[test]
    fn concurrent_operations() {
        workers::block_on(async {
            use std::sync::atomic::AtomicUsize;

            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = Arc::new(DefraBlockstore::new(store, false));

            let mut cids = Vec::new();
            for i in 0..50 {
                let data = format!("preload {}", i);
                let cid = cid_from_data(data.as_bytes());
                blockstore.put(&cid, data.as_bytes()).await.unwrap();
                cids.push(cid);
            }

            let success_count = Arc::new(AtomicUsize::new(0));
            let mut handles = Vec::new();

            for &cid in &cids {
                let bs = blockstore.clone();
                let counter = success_count.clone();
                handles.push(
                    workers::spawn(None, "test", move || async move {
                        for _ in 0..10 {
                            if bs.get(&cid).await.is_ok() {
                                counter.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    })
                    .unwrap(),
                );
            }

            for i in 0..20 {
                let bs = blockstore.clone();
                let counter = success_count.clone();
                handles.push(
                    workers::spawn(None, "test", move || async move {
                        let data = format!("concurrent write {}", i);
                        let cid = cid_from_data(data.as_bytes());
                        if bs.put(&cid, data.as_bytes()).await.is_ok() {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    })
                    .unwrap(),
                );
            }

            for handle in handles {
                handle.await.unwrap();
            }

            let total_ops = success_count.load(Ordering::Relaxed);
            assert!(
                total_ops >= 500,
                "Expected at least 500 successful ops, got {}",
                total_ops
            );
        })
        .unwrap()
    }
}

mod error_paths {
    use super::*;

    #[test]
    fn all_cids_skips_malformed_keys() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store.clone(), false);

            let cid1 = test_cid();
            let cid2 = test_cid2();
            blockstore.put(&cid1, b"data1").await.unwrap();
            blockstore.put(&cid2, b"data2").await.unwrap();

            let malformed_key = vec![0xAA, 0xBB, 0xCC, 0xDD];
            {
                let mut txn = store.new_txn(false).await.unwrap();
                let mut namespaced_key = vec![b'b'];
                namespaced_key.extend_from_slice(&malformed_key);
                txn.set(&namespaced_key, b"garbage").await.unwrap();
                txn.commit().await.unwrap();
            }

            let cids = blockstore.all_cids().await.unwrap();
            assert_eq!(
                cids.len(),
                2,
                "Should return only valid CIDs, skipping malformed key"
            );
            assert!(cids.contains(&cid1));
            assert!(cids.contains(&cid2));
        })
        .unwrap()
    }

    #[test]
    fn cid_bytes_cannot_start_with_merge_prefix() {
        workers::block_on(async {
            use storage::keys::blockstore::ToMergeIndexKey;

            let cidv1 = test_cid();
            let cidv1_bytes = cidv1.to_bytes();
            assert_ne!(
                cidv1_bytes[0], b'm',
                "CIDv1 should not start with 'm' - would break is_merge_key filtering"
            );
            assert_eq!(
                cidv1_bytes[0], 0x01,
                "CIDv1 should start with version byte 0x01"
            );
            assert!(!ToMergeIndexKey::is_merge_key(&cidv1_bytes));

            let cidv0 = Cid::from_str("QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n").unwrap();
            let cidv0_bytes = cidv0.to_bytes();
            assert_ne!(
                cidv0_bytes[0], b'm',
                "CIDv0 should not start with 'm' - would break is_merge_key filtering"
            );
            assert_eq!(
                cidv0_bytes[0], 0x12,
                "CIDv0 should start with sha2-256 code 0x12"
            );
            assert!(!ToMergeIndexKey::is_merge_key(&cidv0_bytes));

            let raw_cid = cid_from_data(b"test data");
            let raw_bytes = raw_cid.to_bytes();
            assert_eq!(raw_bytes[0], 0x01, "Raw CIDv1 should start with 0x01");
            assert!(!ToMergeIndexKey::is_merge_key(&raw_bytes));
        })
        .unwrap()
    }
}

/// Bytes a reader holds never change, whatever the store does after the read.
///
/// A read is zero-copy: it hands back `Bytes` over a regolith `DbSlice`, a
/// refcount on whatever owns those bytes, a memtable arena or an SSTable
/// block. Holding the slice keeps that owner alive, so a block a caller holds
/// (a DAG fetch queue, a merge batch) reads the same when the block is
/// deleted, when other blocks are written, and when the memtable it came from
/// is flushed and the tables it went to are compacted away.
mod cache_ownership {
    use super::*;

    /// A store whose 16 KiB memtable and two-table L0 trigger make [`churn`]
    /// flush the memtable many times over and compact the tables it flushes.
    fn small_memtable_store() -> Arc<RegolithStore> {
        let mut options = storage::RegolithStoreOptions::memory();
        options.engine = options
            .engine
            .write_buffer_size(16 * 1024)
            .l0_compaction_trigger(2);
        Arc::new(RegolithStore::open_with_options("regolith-memory", options).unwrap())
    }

    /// Writes 512 distinct 1 KiB blocks tagged by `round`, in batches.
    async fn churn(blockstore: &DefraBlockstore<RegolithStore>, round: u8) {
        for batch in 0..16u32 {
            let payloads: Vec<Vec<u8>> = (0..32u32)
                .map(|index| {
                    let mut payload = vec![round; 1024];
                    payload[..4].copy_from_slice(&(batch * 32 + index).to_be_bytes());
                    payload
                })
                .collect();
            let cids: Vec<Cid> = payloads
                .iter()
                .map(|payload| cid_from_data(payload))
                .collect();
            let blocks: Vec<(&Cid, &[u8])> = cids
                .iter()
                .zip(&payloads)
                .map(|(cid, payload)| (cid, payload.as_slice()))
                .collect();
            blockstore.put_many(&blocks).await.unwrap();
        }
    }

    #[test]
    fn bytes_a_reader_holds_never_change_when_the_block_is_deleted() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let writer = DefraBlockstore::new(Arc::clone(&store), false);
            let reader = DefraBlockstore::new(Arc::clone(&store), false);
            let cid = test_cid();
            let data = Bytes::from(vec![0xAB; 4096]);
            writer.put(&cid, &data).await.unwrap();

            let first = reader.get(&cid).await.unwrap().expect("block present");
            let second = reader.get(&cid).await.unwrap().expect("block present");
            writer.delete(&cid).await.unwrap();

            assert_eq!(reader.get(&cid).await.unwrap(), None);
            assert_eq!(first, data, "a held read changed after the delete");
            assert_eq!(second, data, "a held read changed after the delete");
        })
        .unwrap()
    }

    #[test]
    fn bytes_a_reader_holds_never_change_when_the_store_is_written() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(store, false);
            let cid = test_cid();
            let data = Bytes::from(vec![0xCD; 4096]);
            blockstore.put(&cid, &data).await.unwrap();

            let held = blockstore.get(&cid).await.unwrap().expect("block present");
            blockstore.put(&cid, &data).await.unwrap();
            churn(&blockstore, 1).await;

            assert_eq!(held, data, "a held read changed after later writes");
            assert_eq!(blockstore.get(&cid).await.unwrap(), Some(data));
        })
        .unwrap()
    }

    #[test]
    fn bytes_a_reader_holds_never_change_when_the_store_flushes_and_compacts() {
        workers::block_on(async {
            let blockstore = DefraBlockstore::new(small_memtable_store(), false);
            let cid = test_cid();
            let data: Bytes = (0..4096u32)
                .map(|i| (i % 251) as u8)
                .collect::<Vec<_>>()
                .into();
            blockstore.put(&cid, &data).await.unwrap();

            // Read while the block is in the memtable, then flush and compact it.
            let from_memtable = blockstore.get(&cid).await.unwrap().expect("block present");
            churn(&blockstore, 1).await;
            assert_eq!(from_memtable, data, "a held read changed after a flush");

            // Read it again from the tables, then delete it and compact the
            // tables that held it.
            let from_tables = blockstore.get(&cid).await.unwrap().expect("block present");
            blockstore.delete(&cid).await.unwrap();
            churn(&blockstore, 2).await;
            churn(&blockstore, 3).await;

            assert_eq!(blockstore.get(&cid).await.unwrap(), None);
            assert_eq!(from_memtable, data, "a held read changed after compaction");
            assert_eq!(from_tables, data, "a held read changed after compaction");
        })
        .unwrap()
    }

    /// The copy is a copy of the value, not of a truncated or shared view of
    /// it, so a large block round-trips through the cache intact.
    #[test]
    fn a_large_cached_block_round_trips_intact() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let blockstore = DefraBlockstore::new(Arc::clone(&store), false);
            let cid = test_cid2();
            let data: Bytes = (0..64 * 1024u32)
                .map(|i| (i % 251) as u8)
                .collect::<Vec<_>>()
                .into();

            blockstore.put(&cid, &data).await.unwrap();
            let reader = DefraBlockstore::new(Arc::clone(&store), false);
            let _ = reader.get(&cid).await.unwrap();
            let cached = reader.get(&cid).await.unwrap().expect("block present");

            assert_eq!(cached.len(), data.len());
            assert_eq!(cached, data);
        })
        .unwrap()
    }
}

/// The blockstore keeps no cache of its own, so every handle over one store
/// sees what the store holds. A handle-local cache answered from what that
/// handle saw last: a block deleted through another handle was still served,
/// a put of it was skipped as already stored, and its merge mark outlived it.
mod one_view_of_the_store {
    use super::*;

    fn two_handles(
        is_p2p: bool,
    ) -> (
        DefraBlockstore<RegolithStore>,
        DefraBlockstore<RegolithStore>,
    ) {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        (
            DefraBlockstore::new(Arc::clone(&store), is_p2p),
            DefraBlockstore::new(store, is_p2p),
        )
    }

    #[test]
    fn a_block_deleted_through_one_handle_is_gone_through_another() {
        workers::block_on(async {
            let (writer, reader) = two_handles(false);
            let cid = test_cid();
            writer.put(&cid, b"block").await.unwrap();
            assert!(reader.get(&cid).await.unwrap().is_some());
            assert!(reader.has(&cid).await.unwrap());

            writer.delete(&cid).await.unwrap();

            assert_eq!(reader.get(&cid).await.unwrap(), None);
            assert!(!reader.has(&cid).await.unwrap());
        })
        .unwrap()
    }

    #[test]
    fn a_put_after_a_delete_through_another_handle_stores_the_block() {
        workers::block_on(async {
            let (writer, reader) = two_handles(false);
            let cid = test_cid();
            reader.put(&cid, b"block").await.unwrap();
            writer.delete(&cid).await.unwrap();

            reader.put(&cid, b"block").await.unwrap();

            assert_eq!(
                writer.get(&cid).await.unwrap(),
                Some(Bytes::from_static(b"block"))
            );
        })
        .unwrap()
    }

    #[test]
    fn a_batch_put_after_a_delete_through_another_handle_stores_the_blocks() {
        workers::block_on(async {
            let (writer, reader) = two_handles(false);
            let (first, second) = (test_cid(), test_cid2());
            let blocks = [
                (&first, b"first".as_slice()),
                (&second, b"second".as_slice()),
            ];
            reader.put_many(&blocks).await.unwrap();
            writer.delete(&first).await.unwrap();

            reader.put_many(&blocks).await.unwrap();

            assert_eq!(
                writer.get(&first).await.unwrap(),
                Some(Bytes::from_static(b"first"))
            );
            assert_eq!(
                writer.get(&second).await.unwrap(),
                Some(Bytes::from_static(b"second"))
            );
        })
        .unwrap()
    }

    #[test]
    fn a_merge_mark_does_not_outlive_a_delete_through_another_handle() {
        workers::block_on(async {
            let (writer, reader) = two_handles(true);
            let cid = test_cid();
            reader.put(&cid, b"block").await.unwrap();
            reader.mark_as_merged(&cid).await.unwrap();
            assert!(reader.is_merged(&cid).await.unwrap());

            writer.delete(&cid).await.unwrap();

            assert!(!reader.is_merged(&cid).await.unwrap());
        })
        .unwrap()
    }
}

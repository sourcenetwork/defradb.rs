use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use cid::Cid;
use ipld_core::{codec::Codec, ipld, ipld::Ipld};
use multihash_codetable::{Code, MultihashDigest};
use rapidhash::RapidHashSet;
use serde_ipld_dagcbor::codec::DagCborCodec;
use storage::RegolithStore;

use super::collect_dag_blocks_within;

async fn put(store: &impl Blockstore, ipld: Ipld) -> Cid {
    let data = DagCborCodec::encode_to_vec(&ipld).unwrap();
    let cid = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
    store.put(&cid, &data).await.unwrap();
    cid
}

/// A chain of `len` blocks named `tag`, oldest first.
async fn chain(store: &impl Blockstore, tag: &str, len: usize) -> Vec<Cid> {
    let mut cids: Vec<Cid> = Vec::with_capacity(len);
    for idx in 0..len as i64 {
        let cid = match cids.last() {
            Some(prev) => put(store, ipld!({ "tag": tag, "idx": idx, "prev": *prev })).await,
            None => put(store, ipld!({ "tag": tag, "idx": idx })).await,
        };
        cids.push(cid);
    }
    cids
}

/// The cut keeps a block when its shortest distance from any root is within
/// the bound, and the shared visited set still serves each block once.
#[tokio::test]
async fn depth_cut_keeps_blocks_within_their_shortest_distance_of_a_root() {
    let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
    let deep = chain(&store, "deep", 4).await;
    let other = chain(&store, "other", 2).await;
    let root = put(&store, ipld!({ "long": deep[3], "short": deep[0] })).await;

    let cases = [
        (vec![root], Some(0), vec![root]),
        (vec![root], Some(1), vec![root, deep[3], deep[0]]),
        (vec![root], Some(2), vec![root, deep[3], deep[2], deep[0]]),
        (
            vec![root],
            None,
            vec![root, deep[3], deep[2], deep[1], deep[0]],
        ),
        (
            vec![deep[3], other[1], root],
            Some(1),
            vec![deep[3], deep[2], other[1], other[0], root, deep[0]],
        ),
    ];
    for (roots, max_depth, expected) in cases {
        let outcome = collect_dag_blocks_within(&store, &roots, max_depth)
            .await
            .unwrap();
        let served: Vec<Cid> = outcome.blocks.iter().map(|(cid, _)| *cid).collect();
        assert_eq!(served.len(), expected.len(), "{max_depth:?}: {served:?}");
        assert_eq!(
            RapidHashSet::from_iter(served),
            RapidHashSet::from_iter(expected)
        );
        assert!(!outcome.truncated());
    }
}

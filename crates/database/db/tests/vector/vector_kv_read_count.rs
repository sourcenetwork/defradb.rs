//! A counting KV adapter counts each entry it reads, and nothing else: what an
//! execute explain reports as a vector search's `indexFetches`.

use std::sync::atomic::{AtomicU64, Ordering};

use db::index::vector::store::{KvNodeStore, Meta, Node, NodeId, VectorNodeStore};
use storage::corekv::Store;
use storage::RegolithStore;

const COLLECTION: u32 = 7;
const INDEX: u32 = 3;
const EPOCH: u32 = 0;

#[test]
fn every_read_counts_once_and_writes_do_not() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        let reads = AtomicU64::new(0);
        let mut kv = KvNodeStore::new(&mut txn, COLLECTION, INDEX, EPOCH).counting_reads(&reads);

        for id in 1..=3u64 {
            kv.put_node(Node::new(NodeId(id), vec![1.0], 0))
                .await
                .unwrap();
        }
        kv.put_meta(Meta {
            entry_point: NodeId(1),
            top_layer: 0,
            live: 3,
            waste: 0,
            rebuilds: 0,
        })
        .await
        .unwrap();
        kv.put_aux(b's', b"a", b"1").await.unwrap();
        kv.put_aux(b's', b"b", b"2").await.unwrap();
        assert_eq!(reads.load(Ordering::Relaxed), 0, "writes are not reads");

        kv.get_node(NodeId(1)).await.unwrap();
        kv.get_node_advisory(NodeId(2)).await.unwrap();
        kv.get_node_for_links(NodeId(3)).await.unwrap();
        kv.get_meta().await.unwrap();
        kv.get_meta_advisory().await.unwrap();
        kv.get_aux(b's', b"a").await.unwrap();
        assert_eq!(reads.load(Ordering::Relaxed), 6, "one per point read");

        kv.iterate_nodes(|_| Ok(())).await.unwrap();
        kv.iterate_aux(b's', b"", |_, _| Ok(())).await.unwrap();
        assert_eq!(
            reads.load(Ordering::Relaxed),
            6 + 3 + 2,
            "one per entry a scan yields"
        );

        let written = kv
            .write_aux_from_nodes(b'x', |node| {
                Ok((node.id.0.to_be_bytes().to_vec(), Vec::new()))
            })
            .await
            .unwrap();
        assert_eq!(written, 3);
        assert_eq!(
            reads.load(Ordering::Relaxed),
            6 + 3 + 2 + 3,
            "a rebuild scan counts every node it yields"
        );

        kv.clear().await.unwrap();
        assert_eq!(
            reads.load(Ordering::Relaxed),
            6 + 3 + 2 + 3 + 1 + 2 + 3 + 3,
            "clear counts every entry in the epoch it scans"
        );
    })
    .unwrap()
}

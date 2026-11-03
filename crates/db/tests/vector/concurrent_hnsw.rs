use db::index::vector::engine::hnsw::Hnsw;
use db::index::vector::params::Params;
use db::index::vector::store::{KvNodeStore, Meta, Node, NodeId, VectorNodeStore};
use defra_core::vector::Metric;
use storage::corekv::Store;
use storage::RegolithStore;

#[test]
fn overlapping_inserts_keep_shared_links_and_metadata_counts() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut seed = store.new_txn(false).await.unwrap();
        {
            let mut nodes = KvNodeStore::new(&mut seed, 1, 1, 0);
            // A high entry point avoids unrelated height promotions in this test.
            nodes
                .put_node(Node::new(NodeId(0), vec![1.0, 0.0], 31))
                .await
                .unwrap();
            nodes
                .put_meta(Meta {
                    entry_point: NodeId(0),
                    top_layer: 31,
                    live: 1,
                    waste: 0,
                    rebuilds: 1,
                })
                .await
                .unwrap();
        }
        seed.commit().await.unwrap();
        let mut a = store.new_txn(false).await.unwrap();
        let mut b = store.new_txn(false).await.unwrap();
        for (txn, id, vector) in [(&mut a, 1, [1.0f32, 0.1]), (&mut b, 2, [1.0f32, -0.1])] {
            let nodes = KvNodeStore::new(txn, 1, 1, 0);
            let mut graph = Hnsw::new(nodes, Metric::Cosine, Params::new(16), 42);
            graph.insert(NodeId(id), &vector).await.unwrap();
        }
        b.commit().await.unwrap();
        a.commit().await.unwrap();
        let mut read = store.new_txn(true).await.unwrap();
        let nodes = KvNodeStore::new(&mut read, 1, 1, 0);
        assert_eq!(nodes.get_meta().await.unwrap().unwrap().live, 3);
        let root = nodes.get_node(NodeId(0)).await.unwrap().unwrap();
        assert_eq!(root.layers[0], vec![NodeId(1), NodeId(2)]);
        let graph = Hnsw::new(nodes, Metric::Cosine, Params::new(16), 42);
        assert_eq!(
            graph
                .search_with_ef(&[1.0f32, 0.0], 3, 64)
                .await
                .unwrap()
                .len(),
            3
        );
    })
    .unwrap()
}

#[test]
fn structural_pruning_conflicts_with_a_new_link() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut seed = store.new_txn(false).await.unwrap();
        KvNodeStore::new(&mut seed, 1, 1, 0)
            .put_node(Node::new(NodeId(0), vec![1.0], 0))
            .await
            .unwrap();
        seed.commit().await.unwrap();
        let mut prune = store.new_txn(false).await.unwrap();
        let mut add = store.new_txn(false).await.unwrap();
        {
            let mut nodes = KvNodeStore::new(&mut prune, 1, 1, 0);
            let mut node = nodes.get_node(NodeId(0)).await.unwrap().unwrap();
            node.layers[0].clear();
            nodes.put_node(node).await.unwrap();
        }
        {
            let mut nodes = KvNodeStore::new(&mut add, 1, 1, 0);
            assert!(nodes.get_node_for_links(NodeId(0)).await.unwrap().is_some());
            nodes.add_link(NodeId(0), 0, NodeId(1)).await.unwrap();
        }
        add.commit().await.unwrap();
        assert!(matches!(
            prune.commit().await,
            Err(storage::Error::TxnConflictDetails(_))
        ));
    })
    .unwrap()
}

use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use storage::RegolithStore;

use super::haves_from_heads;
use crate::sync::car::doc_dag::DocDag;

/// On a 300-update chain the haves are the head, its next three parents,
/// then the composites 4, 8 … 128 back.
#[nuclei::test(threads = 0)]
async fn haves_are_dense_then_doubling() {
    let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
    let mut dag = DocDag::default();
    let mut composites = Vec::new();
    for idx in 0..300 {
        let blocks = dag.update(&["name"], idx);
        for (cid, data) in &blocks {
            store.put(cid, data).await.unwrap();
        }
        composites.push(blocks.last().unwrap().0);
    }
    composites.reverse();

    let haves = haves_from_heads(&store, dag.heads()).await;

    let expected: Vec<_> = [0, 1, 2, 3, 4, 8, 16, 32, 64, 128]
        .iter()
        .map(|back| composites[*back])
        .collect();
    assert_eq!(haves, expected);
}

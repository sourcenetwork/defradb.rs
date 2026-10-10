//! DiskANN: exact until trained, then a FreshVamana graph walked by codes.
//!
//! Recall thresholds hold for the synthetic clustered corpus below; like the
//! IVF-PQ gate, they catch regressions rather than predict a real corpus.

use std::collections::HashSet;

use db::index::vector::engine::ann::VectorIndexEngine;
use db::index::vector::engine::diskann::codec;
use db::index::vector::engine::diskann::{DiskAnn, DiskAnnParams, TRAIN_THRESHOLD};
use db::index::vector::engine::flat::Flat;
use db::index::vector::store::{MemoryNodeStore, NodeId, VectorNodeStore};
use defra_core::vector::Metric;

const SEED: u64 = 0x0D15_CA11;
const DIMENSIONS: usize = 16;
const K: usize = 10;
const QUERIES: usize = 20;
const CORPUS: usize = TRAIN_THRESHOLD as usize + 200;

fn params() -> DiskAnnParams {
    DiskAnnParams {
        r: 24,
        l_build: 64,
        l_search: 64,
        m: 8,
        ..DiskAnnParams::default()
    }
}

fn index() -> DiskAnn<MemoryNodeStore> {
    DiskAnn::try_new(MemoryNodeStore::new(), Metric::Cosine, params(), SEED).unwrap()
}

fn corpus() -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let mut corpus = crate::support::Corpus::new(SEED);
    let vectors = corpus.clustered(CORPUS, DIMENSIONS, 12, 0.3);
    let queries = corpus.clustered(QUERIES, DIMENSIONS, 12, 0.3);
    (vectors, queries)
}

async fn built(vectors: &[Vec<f32>]) -> DiskAnn<MemoryNodeStore> {
    let mut index = index();
    for (i, vector) in vectors.iter().enumerate() {
        index.insert(NodeId(i as u64 + 1), vector).await.unwrap();
    }
    assert!(index.should_build().await.unwrap());
    index.build().await.unwrap();
    index
}

/// recall@K of `index` against an exact scan over the `live` vectors.
async fn recall(
    index: &DiskAnn<MemoryNodeStore>,
    vectors: &[Vec<f32>],
    live: &HashSet<u64>,
    queries: &[Vec<f32>],
) -> f64 {
    let mut flat = Flat::new(MemoryNodeStore::new(), Metric::Cosine);
    for (i, vector) in vectors.iter().enumerate() {
        if live.contains(&(i as u64 + 1)) {
            flat.insert(NodeId(i as u64 + 1), vector).await.unwrap();
        }
    }
    let (mut hit, mut total) = (0, 0);
    for query in queries {
        let want: Vec<NodeId> = flat
            .search(query.as_slice(), K, None)
            .await
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        let got = index.search(query.as_slice(), K, None).await.unwrap();
        hit += got.iter().filter(|n| want.contains(&n.id)).count();
        total += want.len();
    }
    hit as f64 / total as f64
}

fn all(count: usize) -> HashSet<u64> {
    (1..=count as u64).collect()
}

#[tokio::test]
async fn an_untrained_index_is_exact() {
    let (vectors, queries) = corpus();
    let small = &vectors[..200];
    let mut index = index();
    for (i, vector) in small.iter().enumerate() {
        index.insert(NodeId(i as u64 + 1), vector).await.unwrap();
    }
    assert!(!index.should_build().await.unwrap());
    assert!(index.state().await.unwrap().is_none());
    assert_eq!(
        recall(&index, small, &all(small.len()), &queries).await,
        1.0
    );
}

#[tokio::test]
async fn a_trained_graph_meets_the_recall_gate() {
    let (vectors, queries) = corpus();
    let index = built(&vectors).await;
    let state = index.state().await.unwrap().expect("trained");
    assert_eq!(state.live, CORPUS as u64);
    assert_eq!(state.m, 8);

    let recall = recall(&index, &vectors, &all(CORPUS), &queries).await;
    assert!(recall >= 0.9, "recall@{K} is {recall}");
}

/// The memory bound rests on this: a record never holds more than the slack
/// limit of edges, so one read and one record's codes rank any hop. Back-edges
/// may grow a record past `R` without a re-prune, which is what keeps an insert
/// from pruning every full neighbour it links to.
#[tokio::test]
async fn records_stay_within_the_slack_limit() {
    let (vectors, _) = corpus();
    let index = built(&vectors).await;
    let m = index.state().await.unwrap().unwrap().m as usize;
    let limit = params().slack_limit();
    let r = params().r as usize;
    assert!(limit > r);
    let (mut records, mut over_r) = (0, 0);
    index
        .store()
        .iterate_aux(codec::GRAPH, b"", |_, value| {
            let record = codec::decode_record(value, m)?;
            assert!(record.neighbors.len() <= limit);
            assert!(
                !record.neighbors.is_empty(),
                "an isolated node is unreachable"
            );
            records += 1;
            over_r += usize::from(record.neighbors.len() > r);
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(records, CORPUS);
    assert!(over_r > 0, "no record used its slack");
}

#[tokio::test]
async fn an_insert_after_training_is_found() {
    let (vectors, _) = corpus();
    let mut index = built(&vectors).await;
    let fresh = crate::support::Corpus::new(0xF2E5).vector(DIMENSIONS);
    let id = NodeId(CORPUS as u64 + 1);
    index.insert(id, &fresh).await.unwrap();

    let hits = index.search(fresh.as_slice(), K, None).await.unwrap();
    assert_eq!(hits[0].id, id, "a vector is nearest itself");
    assert_eq!(
        index.state().await.unwrap().unwrap().live,
        CORPUS as u64 + 1
    );
}

#[tokio::test]
async fn a_reinsert_replaces_the_vector() {
    let (vectors, _) = corpus();
    let mut index = built(&vectors).await;
    let moved = crate::support::Corpus::new(0x40FE).vector(DIMENSIONS);
    index.insert(NodeId(7), &moved).await.unwrap();

    let hits = index.search(moved.as_slice(), 1, None).await.unwrap();
    assert_eq!(hits[0].id, NodeId(7));
    assert_eq!(
        index.state().await.unwrap().unwrap().live,
        CORPUS as u64,
        "a re-insert is not a new node"
    );
}

#[tokio::test]
async fn deletes_are_never_returned_and_consolidate() {
    let (vectors, queries) = corpus();
    let mut index = built(&vectors).await;
    let mut live = all(CORPUS);
    for id in (1..=CORPUS as u64).step_by(5) {
        assert!(index.delete(NodeId(id)).await.unwrap());
        live.remove(&id);
    }
    for query in &queries {
        for hit in index.search(query.as_slice(), K, None).await.unwrap() {
            assert!(live.contains(&hit.id.0), "returned deleted {:?}", hit.id);
        }
    }

    assert!(
        index.should_build().await.unwrap(),
        "a fifth deleted is due"
    );
    index.build().await.unwrap();
    let state = index.state().await.unwrap().unwrap();
    assert_eq!((state.live, state.deleted), (live.len() as u64, 0));
    assert!(!index.should_build().await.unwrap());

    let recall = recall(&index, &vectors, &live, &queries).await;
    assert!(recall >= 0.9, "recall@{K} after consolidation is {recall}");
}

/// FreshVamana's claim (paper Fig. 2): with `alpha > 1`, recall holds across
/// repeated cycles of deleting and re-inserting a slice of the index.
#[tokio::test]
async fn recall_is_stable_across_update_cycles() {
    let (vectors, queries) = corpus();
    let mut index = built(&vectors).await;
    let live = all(CORPUS);
    let first = recall(&index, &vectors, &live, &queries).await;

    for cycle in 0..5u64 {
        let slice: Vec<u64> = (1..=CORPUS as u64)
            .filter(|id| (id + cycle) % 10 == 0)
            .collect();
        for id in &slice {
            index.delete(NodeId(*id)).await.unwrap();
        }
        index.build().await.unwrap();
        for id in &slice {
            index
                .insert(NodeId(*id), &vectors[*id as usize - 1])
                .await
                .unwrap();
        }
        let now = recall(&index, &vectors, &live, &queries).await;
        assert!(
            now >= first - 0.05,
            "cycle {cycle}: recall fell from {first} to {now}"
        );
    }
}

#[tokio::test]
async fn filtered_search_returns_only_admitted_nodes() {
    let (vectors, queries) = corpus();
    let index = built(&vectors).await;
    let even = |id: NodeId| id.0.is_multiple_of(2);
    for query in &queries {
        let hits = index
            .search_where(query.as_slice(), K, None, &even)
            .await
            .unwrap();
        assert_eq!(hits.len(), K);
        assert!(hits.iter().all(|hit| even(hit.id)));
    }

    // Too selective for the walk to fill `K`: the exact scan answers instead.
    let only = |id: NodeId| id.0 == 42;
    let hits = index
        .search_where(queries[0].as_slice(), K, None, &only)
        .await
        .unwrap();
    assert_eq!(hits.iter().map(|h| h.id).collect::<Vec<_>>(), [NodeId(42)]);
}

#[tokio::test]
async fn a_query_of_the_wrong_width_is_refused() {
    let (vectors, _) = corpus();
    let index = built(&vectors).await;
    let err = index
        .search(&[1.0f32; DIMENSIONS + 1], K, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("dimension"), "got: {err}");
}

#[test]
fn a_dot_product_metric_is_refused() {
    let err =
        DiskAnn::try_new(MemoryNodeStore::new(), Metric::NegativeDot, params(), SEED).unwrap_err();
    assert!(err.to_string().contains("squared distance"), "got: {err}");
}

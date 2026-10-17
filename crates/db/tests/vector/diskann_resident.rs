//! DiskANN with resident codes: records hold neighbour ids only, each node's
//! code has its own key, and searches read codes from a shared cache.

use std::sync::Arc;

use crate::diskann_engine::{all, corpus, count, params, recall, CORPUS, K, SEED};
use db::index::vector::engine::ann::VectorIndexEngine;
use db::index::vector::engine::diskann::{codec, CodeCache, Codes, DiskAnn, DiskAnnParams};
use db::index::vector::store::{MemoryNodeStore, NodeId, VectorNodeStore};
use defra_core::vector::Metric;

async fn built(vectors: &[Vec<f32>]) -> DiskAnn<MemoryNodeStore> {
    let params = DiskAnnParams {
        resident_codes: true,
        ..params()
    };
    let mut index = DiskAnn::try_new(MemoryNodeStore::new(), Metric::Cosine, params, SEED).unwrap();
    for (i, vector) in vectors.iter().enumerate() {
        index.insert(NodeId(i as u64 + 1), vector).await.unwrap();
    }
    index.build().await.unwrap();
    index
}

async fn stored_code(index: &DiskAnn<MemoryNodeStore>, id: u64) -> Vec<u8> {
    let key = codec::record_key(NodeId(id));
    let code = index.store().get_aux(codec::CODE, &key).await.unwrap();
    code.unwrap().to_vec()
}

#[test]
fn records_hold_ids_and_recall_holds() {
    workers::block_on(async {
        let (vectors, queries) = corpus();
        let index = built(&vectors).await;

        assert_eq!(count(&index, codec::CODE).await, CORPUS);
        index
            .store()
            .iterate_aux(codec::GRAPH, b"", |_, record| {
                codec::decode_ids(record)?;
                Ok(())
            })
            .await
            .unwrap();
        let recall = recall(&index, &vectors, &all(CORPUS), &queries).await;
        assert!(recall >= 0.9, "recall@{K} is {recall}");
    })
    .unwrap()
}

#[test]
fn consolidation_drops_codes_with_records() {
    workers::block_on(async {
        let (vectors, queries) = corpus();
        let mut index = built(&vectors).await;
        let deleted: Vec<u64> = (1..=130).collect();
        for id in &deleted {
            index.delete(NodeId(*id)).await.unwrap();
        }
        index.build().await.unwrap();

        let live = CORPUS - deleted.len();
        assert_eq!(count(&index, codec::GRAPH).await, live);
        assert_eq!(count(&index, codec::CODE).await, live);
        let survivors = (131..=CORPUS as u64).collect();
        let recall = recall(&index, &vectors, &survivors, &queries).await;
        assert!(recall >= 0.9, "recall@{K} after consolidation is {recall}");
    })
    .unwrap()
}

/// An update rewrites the node's code in the merge; the next search sees the
/// merge's new generation and reloads, rather than walking by the old code.
#[test]
fn the_cache_reloads_after_a_merge() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let cache = Arc::new(CodeCache::default());
        let mut index = built(&vectors).await.with_codes(cache.clone());
        index.search(vectors[6].as_slice(), K, None).await.unwrap();
        assert_eq!(cache.get(NodeId(7)), Some(stored_code(&index, 7).await));

        let moved = crate::diskann_streaming::fresh(0x40FE);
        index.insert(NodeId(7), &moved).await.unwrap();
        index.build().await.unwrap();
        let hits = index.search(moved.as_slice(), K, None).await.unwrap();

        assert_eq!(hits[0].id, NodeId(7));
        assert_eq!(cache.get(NodeId(7)), Some(stored_code(&index, 7).await));
    })
    .unwrap()
}

/// A reload that finishes after a newer one must not overwrite it.
#[test]
fn an_older_reload_does_not_replace_a_newer_one() {
    let codes = |code: u8| {
        let codes = Codes::default();
        codes.insert(NodeId(1), vec![code]);
        codes
    };
    let cache = CodeCache::default();

    cache.replace(2, codes(2));
    cache.replace(1, codes(1));

    assert!(cache.is_current(2));
    assert_eq!(cache.get(NodeId(1)), Some(vec![2]));
}

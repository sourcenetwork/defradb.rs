//! Back-edges on a resident-codes DiskANN record: the graph a resident-codes
//! index grows is the graph the records-with-codes layout grows, record for
//! record, and a neighbour consolidated away is still dropped from a record a
//! back-edge rewrites.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::diskann_engine::{corpus, params, K, SEED};
use db::index::vector::engine::ann::VectorIndexEngine;
use db::index::vector::engine::diskann::{codec, CodeCache, DiskAnn, DiskAnnParams};
use db::index::vector::store::{MemoryNodeStore, NodeId, VectorNodeStore};
use defra_core::vector::Metric;

type Records = BTreeMap<Vec<u8>, Vec<u8>>;

fn engine(resident_codes: bool) -> DiskAnn<MemoryNodeStore> {
    let params = DiskAnnParams {
        resident_codes,
        ..params()
    };
    DiskAnn::try_new(MemoryNodeStore::new(), Metric::Cosine, params, SEED).unwrap()
}

async fn aux(index: &DiskAnn<MemoryNodeStore>, kind: u8) -> Records {
    let mut records = Records::new();
    index
        .store()
        .iterate_aux(kind, b"", |key, value| {
            records.insert(key.to_vec(), value.to_vec());
            Ok(())
        })
        .await
        .unwrap();
    records
}

/// Trains, merges inserts in two rounds, consolidates a run of deletes, then
/// merges more inserts: every way a back-edge reaches a record.
async fn grow(index: &mut DiskAnn<MemoryNodeStore>, vectors: &[Vec<f32>]) {
    let (first, rest) = vectors.split_at(vectors.len() - 400);
    for (i, vector) in first.iter().enumerate() {
        index.insert(NodeId(i as u64 + 1), vector).await.unwrap();
    }
    index.build().await.unwrap();
    for (round, extra) in rest.chunks(200).enumerate() {
        for (i, vector) in extra.iter().enumerate() {
            let id = (first.len() + round * 200 + i) as u64 + 1;
            index.insert(NodeId(id), vector).await.unwrap();
        }
        while index.should_build().await.unwrap() {
            index.build().await.unwrap();
        }
        if round == 0 {
            for id in 1..=130 {
                index.delete(NodeId(id)).await.unwrap();
            }
            while index.should_build().await.unwrap() {
                index.build().await.unwrap();
            }
        }
    }
}

fn corpus_with_extra() -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let (mut vectors, queries) = corpus();
    vectors.extend((0..400).map(|i| crate::diskann_streaming::fresh(0x2212 + i)));
    (vectors, queries)
}

/// A resident-codes index, with and without the shared code cache, writes
/// the records the records-with-codes layout writes, with their codes moved
/// to their own keys, and answers every query the same.
#[test]
fn resident_records_are_the_records_with_codes_left_out() {
    workers::block_on(async {
        let (vectors, queries) = corpus_with_extra();
        let mut with_codes = engine(false);
        let mut resident = engine(true);
        let mut cached = engine(true).with_codes(Arc::new(CodeCache::default()));
        grow(&mut with_codes, &vectors).await;
        grow(&mut resident, &vectors).await;
        grow(&mut cached, &vectors).await;

        let m = with_codes.state().await.unwrap().unwrap().m as usize;
        let records = aux(&with_codes, codec::GRAPH).await;
        let mut ids = Records::new();
        let mut codes = Records::new();
        for (key, bytes) in &records {
            let record = codec::decode_record(bytes, m).unwrap();
            ids.insert(key.clone(), codec::encode_ids(&record.neighbors));
            codes.insert(key.clone(), record.code);
        }
        assert!(records.len() > 1000, "{} records", records.len());

        for index in [&resident, &cached] {
            assert_eq!(aux(index, codec::GRAPH).await, ids);
            assert_eq!(aux(index, codec::CODE).await, codes);
            for query in &queries {
                assert_eq!(
                    index.search(query.as_slice(), K, None).await.unwrap(),
                    with_codes.search(query.as_slice(), K, None).await.unwrap(),
                );
            }
        }
    })
    .unwrap()
}

/// A neighbour whose code is gone, as a consolidation that raced the record's
/// last rewrite leaves it, is dropped from every record a back-edge rewrites
/// and stays only in records no back-edge reached.
#[test]
fn a_back_edge_drops_a_neighbour_consolidated_away() {
    workers::block_on(async {
        let (vectors, _) = corpus_with_extra();
        let (built, extra) = vectors.split_at(vectors.len() - 400);
        let mut index = engine(true);
        for (i, vector) in built.iter().enumerate() {
            index.insert(NodeId(i as u64 + 1), vector).await.unwrap();
        }
        index.build().await.unwrap();

        let entry = index.state().await.unwrap().unwrap().entry.unwrap();
        let before = aux(&index, codec::GRAPH).await;
        let mut in_edges: BTreeMap<NodeId, usize> = BTreeMap::new();
        for bytes in before.values() {
            for id in codec::decode_ids(bytes).unwrap() {
                *in_edges.entry(id).or_default() += 1;
            }
        }
        let gone = in_edges
            .iter()
            .filter(|(id, _)| **id != entry)
            .max_by_key(|(_, n)| **n)
            .map(|(id, _)| *id)
            .unwrap();
        let key = codec::record_key(gone);
        index
            .store_mut()
            .delete_aux(codec::GRAPH, &key)
            .await
            .unwrap();
        index
            .store_mut()
            .delete_aux(codec::CODE, &key)
            .await
            .unwrap();

        for (i, vector) in extra.iter().enumerate() {
            let id = (built.len() + i) as u64 + 1;
            index.insert(NodeId(id), vector).await.unwrap();
        }
        index.build().await.unwrap();

        let mut rewritten = 0;
        for (key, bytes) in aux(&index, codec::GRAPH).await {
            let ids = codec::decode_ids(&bytes).unwrap();
            match before.get(&key) {
                Some(old) if *old == bytes => {}
                Some(old) => {
                    assert!(!ids.contains(&gone), "record {key:?} kept {gone:?}");
                    rewritten += usize::from(codec::decode_ids(old).unwrap().contains(&gone));
                }
                None => assert!(!ids.contains(&gone)),
            }
        }
        assert!(
            rewritten > 0,
            "no back-edge reached a record naming {gone:?}"
        );
    })
    .unwrap()
}

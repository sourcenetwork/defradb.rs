//! Resident-codes records keyed by cluster: each record sits under its
//! cluster's tag, clusters stay within their capacity, a node keeps its
//! cluster, a search finds a record whatever cluster the cache names, and an
//! index written before clusters keeps working.

use std::sync::Arc;

use rapidhash::{HashMapExt, RapidHashMap};

use crate::diskann_engine::{all, corpus, count, recall, Counting, CORPUS, K, SEED};
use crate::diskann_resident::{built, stored_code};
use db::index::vector::engine::ann::VectorIndexEngine;
use db::index::vector::engine::diskann::{codec, CodeCache, Codes, DiskAnn};
use db::index::vector::store::{MemoryNodeStore, NodeId, VectorNodeStore};
use defra_core::vector::Metric;

/// Every record's key and its cluster, if it has one.
async fn keys(index: &DiskAnn<MemoryNodeStore>) -> Vec<(NodeId, Option<u32>)> {
    let mut keys = Vec::new();
    index
        .store()
        .iterate_aux(codec::GRAPH, b"", |key, _| {
            let cluster =
                (key.len() == 12).then(|| u32::from_be_bytes(key[..4].try_into().unwrap()));
            keys.push((codec::graph_id(key)?, cluster));
            Ok(())
        })
        .await
        .unwrap();
    keys
}

/// Rewrites every record and code into the layout written before clusters:
/// records keyed by id, codes without a cluster.
async fn unclustered(index: &mut DiskAnn<MemoryNodeStore>) {
    let mut entries = Vec::new();
    for kind in [codec::GRAPH, codec::CODE] {
        index
            .store()
            .iterate_aux(kind, b"", |key, value| {
                entries.push((kind, key.to_vec(), value.to_vec()));
                Ok(())
            })
            .await
            .unwrap();
    }
    let m = index.state().await.unwrap().unwrap().m as usize;
    for (kind, key, value) in entries {
        index.store_mut().delete_aux(kind, &key).await.unwrap();
        let id = codec::graph_id(&key).unwrap();
        let value = match kind {
            codec::CODE => codec::decode_code(&value, m).unwrap().0.to_vec(),
            _ => value,
        };
        index
            .store_mut()
            .put_aux(kind, &codec::record_key(id), &value)
            .await
            .unwrap();
    }
}

#[test]
fn records_sit_under_their_cluster_and_clusters_stay_within_capacity() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let index = built(&vectors).await;
        let capacity = index.params().cluster_capacity();

        let mut sizes: RapidHashMap<u32, usize> = RapidHashMap::new();
        for (id, cluster) in keys(&index).await {
            let cluster = cluster.expect("a record written now has a cluster");
            assert_eq!(
                stored_code(&index, id.0).await.1,
                Some(cluster),
                "node {id:?}"
            );
            *sizes.entry(cluster).or_default() += 1;
        }
        assert!(sizes.values().all(|size| *size <= capacity), "{sizes:?}");
        let mean = CORPUS as f64 / sizes.len() as f64;
        assert!(
            mean >= 32.0,
            "clusters average {mean} records of {capacity}"
        );
    })
    .unwrap()
}

#[test]
fn graph_keys_and_codes_round_trip() {
    let id = NodeId(0x0102_0304_0506_0708);
    let tag = codec::cluster_tag(NodeId(0xffff_ffff_ffff_ffff));
    assert_eq!(tag, codec::CLUSTER_TAG_MASK);
    assert_ne!(tag, codec::NO_CLUSTER);

    assert_eq!(&*codec::graph_key(id, None), &codec::record_key(id));
    let key = codec::graph_key(id, Some(tag));
    assert_eq!(key.len(), 12);
    assert_eq!(codec::graph_id(&key).unwrap(), id);
    assert_eq!(codec::graph_id(&codec::graph_key(id, None)).unwrap(), id);
    assert!(codec::graph_id(&[1, 2, 3]).is_err());

    let code = [9u8, 8, 7];
    for cluster in [None, Some(0), Some(tag)] {
        let value = codec::encode_code(&code, cluster);
        assert_eq!(codec::decode_code(&value, 3).unwrap(), (&code[..], cluster));
    }
    assert!(codec::decode_code(&[1, 2], 3).is_err());
    assert!(codec::decode_code(&codec::encode_code(&code, Some(u32::MAX)), 3).is_err());
}

#[test]
fn a_node_linked_again_keeps_its_cluster() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors).await;
        let before = stored_code(&index, 7).await.1;

        let moved = crate::diskann_streaming::fresh(0x40FE);
        index.insert(NodeId(7), &moved).await.unwrap();
        index.build().await.unwrap();

        assert_eq!(stored_code(&index, 7).await.1, before);
        let records: Vec<_> = keys(&index)
            .await
            .into_iter()
            .filter(|(id, _)| *id == NodeId(7))
            .collect();
        assert_eq!(records, vec![(NodeId(7), before)]);
        assert_eq!(count(&index, codec::GRAPH).await, CORPUS);
        let hits = index.search(moved.as_slice(), K, None).await.unwrap();
        assert_eq!(hits[0].id, NodeId(7));
    })
    .unwrap()
}

/// How many graph records searches for `queries` find, over a copy of
/// `index`'s store, reading clusters through `codes` where given.
async fn records_found(
    index: &DiskAnn<MemoryNodeStore>,
    codes: Option<Arc<CodeCache>>,
    queries: &[Vec<f32>],
) -> usize {
    let store = Counting::new(index.store().clone());
    let mut counted = DiskAnn::try_new(store, Metric::Cosine, index.params(), SEED).unwrap();
    if let Some(codes) = codes {
        counted = counted.with_codes(codes);
    }
    for query in queries {
        counted.search(query, K, None).await.unwrap();
    }
    counted
        .store()
        .record_hits
        .load(std::sync::atomic::Ordering::Relaxed)
}

/// A cache loaded at another generation can name a cluster no record of the
/// snapshot sits under; the search reads the node's own and still walks it.
/// The walk itself must find every record it finds with the right clusters:
/// recall alone would hold without it, through the exact scan a search runs
/// when its walk comes back short.
#[test]
fn a_search_finds_records_whatever_cluster_the_cache_names() {
    workers::block_on(async {
        let (vectors, queries) = corpus();
        let index = built(&vectors).await;
        let right = records_found(&index, None, &queries).await;
        let generation = index.state().await.unwrap().unwrap().generation;
        let mut codes = Codes::default();
        for id in 1..=CORPUS as u64 {
            let (code, cluster) = stored_code(&index, id).await;
            let wrong = cluster.map(|tag| (tag + 1) & codec::CLUSTER_TAG_MASK);
            codes.insert(NodeId(id), &code, wrong).unwrap();
        }
        let cache = Arc::new(CodeCache::default());
        cache.replace(generation, codes);
        let wrong = records_found(&index, Some(cache.clone()), &queries).await;
        assert!(right > 0, "the walks found no record");
        assert_eq!(
            wrong, right,
            "walks with every cached cluster wrong found {wrong} records of {right}"
        );
        let index = index.with_codes(cache);

        let recall = recall(&index, &vectors, &all(CORPUS), &queries).await;
        assert!(
            recall >= 0.9,
            "recall@{K} with every cached cluster wrong is {recall}"
        );
    })
    .unwrap()
}

/// An index written before clusters searches, merges, updates and
/// consolidates as it did; nodes linked after it take clustered keys beside
/// the old ones.
#[test]
fn an_index_written_before_clusters_keeps_working() {
    workers::block_on(async {
        let (vectors, queries) = corpus();
        let mut index = built(&vectors).await;
        unclustered(&mut index).await;
        let recall_before = recall(&index, &vectors, &all(CORPUS), &queries).await;
        assert!(recall_before >= 0.9, "recall@{K} is {recall_before}");

        let moved = crate::diskann_streaming::fresh(0x40FE);
        index.insert(NodeId(7), &moved).await.unwrap();
        let added = crate::diskann_streaming::fresh(0x51AB);
        let new = NodeId(CORPUS as u64 + 1);
        index.insert(new, &added).await.unwrap();
        index.build().await.unwrap();

        let keys = keys(&index).await;
        assert!(
            keys.contains(&(NodeId(7), None)),
            "an old node keeps its key"
        );
        assert!(keys
            .iter()
            .any(|(id, cluster)| *id == new && cluster.is_some()));
        assert_eq!(keys.len(), CORPUS + 1);
        let hits = index.search(moved.as_slice(), K, None).await.unwrap();
        assert_eq!(hits[0].id, NodeId(7));

        let deleted: Vec<u64> = (1..=130).collect();
        for id in &deleted {
            index.delete(NodeId(*id)).await.unwrap();
        }
        index.delete(new).await.unwrap();
        index.build().await.unwrap();
        let live = CORPUS - deleted.len();
        assert_eq!(count(&index, codec::GRAPH).await, live);
        assert_eq!(count(&index, codec::CODE).await, live);
    })
    .unwrap()
}

/// A stale cache cannot make a merge treat a re-clustered node as new. The
/// node is consolidated from the old layout, linked fresh under a cluster,
/// then linked again while the cache still names its bare id key.
#[test]
fn a_reclustered_node_linked_again_has_one_record() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors).await;
        unclustered(&mut index).await;
        let (old_code, old_cluster) = stored_code(&index, 7).await;
        assert_eq!(old_cluster, None);

        for id in 1..=130 {
            index.delete(NodeId(id)).await.unwrap();
        }
        index.build().await.unwrap();
        assert!(!keys(&index).await.iter().any(|(id, _)| *id == NodeId(7)));

        let first = crate::diskann_streaming::fresh(0x40FE);
        index
            .insert(NodeId(CORPUS as u64 + 1), &first)
            .await
            .unwrap();
        index.build().await.unwrap();
        index.insert(NodeId(7), &first).await.unwrap();
        index.build().await.unwrap();
        let linked = stored_code(&index, 7).await.1;
        assert!(linked.is_some());
        assert_ne!(linked, Some(codec::cluster_tag(NodeId(7))));

        let generation = index.state().await.unwrap().unwrap().generation;
        let mut stale = Codes::default();
        stale.insert(NodeId(7), &old_code, None).unwrap();
        let cache = Arc::new(CodeCache::default());
        cache.replace(generation, stale);
        index = index.with_codes(cache);

        let second = crate::diskann_streaming::fresh(0x51AB);
        index.insert(NodeId(7), &second).await.unwrap();
        index.build().await.unwrap();

        let records: Vec<_> = keys(&index)
            .await
            .into_iter()
            .filter(|(id, _)| *id == NodeId(7))
            .collect();
        assert_eq!(records.len(), 1, "{records:?}");
        assert!(records[0].1.is_some(), "{records:?}");
    })
    .unwrap()
}

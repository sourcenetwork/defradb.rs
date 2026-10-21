//! IVF recall on a real corpus: AG News embedded with all-MiniLM-L6-v2.
//!
//! ```text
//! IVF_CORPUS_DIR=<dir with base.f32, queries.f32, gt10.u32> \
//!   cargo test --release -p db --test vector -- ivf_real_corpus --ignored --nocapture
//! ```

use std::time::Instant;

use db::index::vector::engine::ann::VectorIndexEngine;
use db::index::vector::engine::ivf::flat::{IvfFlat, IvfFlatParams};
use db::index::vector::engine::ivf::pq::{IvfPq, IvfPqParams};
use db::index::vector::store::{MemoryNodeStore, NodeId};
use defra_core::vector::Metric;

const DIMENSIONS: usize = 384;
const K: usize = 10;
const QUERIES: usize = 300;
const NPROBES: [usize; 6] = [4, 8, 16, 19, 32, 64];

fn read_f32(path: &str) -> Vec<Vec<f32>> {
    let bytes = std::fs::read(path).unwrap();
    let floats: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    floats
        .as_chunks::<DIMENSIONS>()
        .0
        .iter()
        .map(|v| v.to_vec())
        .collect()
}

struct Corpus {
    base: Vec<Vec<f32>>,
    queries: Vec<Vec<f32>>,
    truth: Vec<Vec<u64>>,
}

fn corpus() -> Corpus {
    let dir = std::env::var("IVF_CORPUS_DIR").expect("IVF_CORPUS_DIR");
    let base = read_f32(&format!("{dir}/base.f32"));
    let mut queries = read_f32(&format!("{dir}/queries.f32"));
    queries.truncate(QUERIES);
    let ids: Vec<u64> = std::fs::read(format!("{dir}/gt10.u32"))
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u64::from(u32::from_le_bytes(*b)) + 1)
        .collect();
    let truth = ids
        .as_chunks::<K>()
        .0
        .iter()
        .take(QUERIES)
        .map(|t| t.to_vec())
        .collect();
    Corpus {
        base,
        queries,
        truth,
    }
}

async fn sweep<E: VectorIndexEngine>(label: &str, index: &E, corpus: &Corpus) {
    for nprobe in NPROBES {
        let started = Instant::now();
        let mut hit = 0usize;
        for (query, want) in corpus.queries.iter().zip(&corpus.truth) {
            let got = index
                .search(query.as_slice(), K, Some(nprobe))
                .await
                .unwrap();
            hit += got.iter().filter(|n| want.contains(&n.id.0)).count();
        }
        let per_query = started.elapsed().as_secs_f64() * 1e3 / corpus.queries.len() as f64;
        println!(
            "{label:>16} nprobe={nprobe:>3} recall@10={:.4} {per_query:>7.2} ms/query",
            hit as f64 / (K * corpus.queries.len()) as f64
        );
    }
}

#[test]
#[ignore = "measurement; needs IVF_CORPUS_DIR"]
fn ivf_real_corpus() {
    workers::block_on(async {
        let corpus = corpus();
        println!(
            "corpus {} x {DIMENSIONS}, {} queries",
            corpus.base.len(),
            corpus.queries.len()
        );

        let mut pq = IvfPq::try_new(
            MemoryNodeStore::new(),
            Metric::Cosine,
            IvfPqParams::default(),
            7,
        )
        .unwrap();
        for (i, v) in corpus.base.iter().enumerate() {
            pq.insert(NodeId(i as u64 + 1), v).await.unwrap();
        }
        let started = Instant::now();
        let report = pq.build().await.unwrap();
        println!(
            "ivfpq build {:.1}s nlist={} m={}",
            started.elapsed().as_secs_f64(),
            report.state.nlist,
            report.state.m
        );
        sweep("ivfpq", &pq, &corpus).await;
        drop(pq);

        let mut flat = IvfFlat::try_new(
            MemoryNodeStore::new(),
            Metric::Cosine,
            IvfFlatParams::default(),
            7,
        )
        .unwrap();
        for (i, v) in corpus.base.iter().enumerate() {
            flat.insert(NodeId(i as u64 + 1), v).await.unwrap();
        }
        let started = Instant::now();
        flat.build().await.unwrap();
        println!("ivfflat build {:.1}s", started.elapsed().as_secs_f64());
        sweep("ivfflat", &flat, &corpus).await;
    })
    .unwrap()
}

/// What reranking `pool` candidates costs in node reads against regolith.
#[test]
#[ignore = "measurement; needs IVF_CORPUS_DIR"]
fn rerank_read_cost() {
    workers::block_on(async {
        use db::index::vector::store::{KvNodeStore, VectorNodeStore};
        use storage::corekv::{Store, Txn};
        use storage::RegolithStore;

        let corpus = corpus();
        let dir = std::env::temp_dir().join(format!("rerank-cost-{}", std::process::id()));
        for (label, store) in [
            ("memory", RegolithStore::in_memory().unwrap()),
            ("disk", RegolithStore::open(&dir).unwrap()),
        ] {
            let mut write: Box<dyn Txn> = store.new_txn(false).await.unwrap();
            {
                let mut nodes = KvNodeStore::new(&mut write, 1, 1, 0);
                for (i, v) in corpus.base.iter().enumerate() {
                    nodes
                        .put_node(db::index::vector::store::Node::new(
                            NodeId(i as u64 + 1),
                            v.clone(),
                            0,
                        ))
                        .await
                        .unwrap();
                }
            }
            write.commit().await.unwrap();

            let mut read: Box<dyn Txn> = store.new_txn(true).await.unwrap();
            let nodes = KvNodeStore::new(&mut read, 1, 1, 0);
            let mut seed = 0x9E37_79B9_7F4A_7C15u64;
            for pool in [10usize, 50, 100] {
                let started = Instant::now();
                for _ in 0..corpus.queries.len() {
                    for _ in 0..pool {
                        seed ^= seed << 13;
                        seed ^= seed >> 7;
                        seed ^= seed << 17;
                        let id = seed % corpus.base.len() as u64 + 1;
                        nodes.get_node(NodeId(id)).await.unwrap().unwrap();
                    }
                }
                println!(
                    "{label:>6} pool={pool:>3} {:.3} ms/query",
                    started.elapsed().as_secs_f64() * 1e3 / corpus.queries.len() as f64
                );
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    })
    .unwrap()
}

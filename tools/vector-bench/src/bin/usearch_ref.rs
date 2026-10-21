//! In-memory usearch HNSW on the same corpus, queries and exact top 10 as
//! `vector_disk_cost`, for scale.
//!
//! `usearch_ref <base.f32> <bench dir> <M> <ef_construction>`; the bench dir
//! is any `vector_disk_cost` build, which holds `queries.f32`, `truth.u64`, `dims`
//! and `count`; the base file is cut to that count.

use std::path::Path;
use std::time::Instant;

use rayon::prelude::*;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};
use vector_bench::{hits, read_base, read_f32, read_u64, rss};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, base, bench, m, efc] = args.as_slice() else {
        panic!("usage: usearch_ref <base.f32> <bench dir> <M> <ef_construction>");
    };
    let (m, efc): (usize, usize) = (m.parse().unwrap(), efc.parse().unwrap());
    let bench = Path::new(bench);
    let (dims, base) = read_base(Path::new(base), bench);
    let queries = read_f32(&bench.join("queries.f32"), dims);
    let truth = read_u64(&bench.join("truth.u64"));
    let opts = IndexOptions {
        dimensions: dims,
        metric: MetricKind::Cos,
        quantization: ScalarKind::F32,
        connectivity: m,
        expansion_add: efc,
        expansion_search: 64,
        multi: false,
    };
    let index = Index::new(&opts).unwrap();
    index.reserve(base.len()).unwrap();
    let t = Instant::now();
    base.par_iter()
        .enumerate()
        .for_each(|(i, v)| index.add(i as u64 + 1, v).unwrap());
    println!(
        "BUILD usearch M={m} efC={efc} threads={} in {:.1}s; index_mem={}MiB {}",
        rayon::current_num_threads(),
        t.elapsed().as_secs_f64(),
        index.memory_usage() >> 20,
        rss()
    );
    drop(base);
    for ef in [32usize, 64, 128, 256, 512] {
        index.change_expansion_search(ef);
        let mut lat = Vec::new();
        let mut found = 0;
        for (i, q) in queries.iter().enumerate() {
            let s = Instant::now();
            let got = index.search(q, 10).unwrap();
            lat.push(s.elapsed().as_nanos());
            found += hits(&truth, i, &got.keys);
        }
        lat.sort();
        let ms = |n: u128| n as f64 / 1e6;
        let t = Instant::now();
        let total: usize = (0..8)
            .into_par_iter()
            .map(|_| {
                for q in &queries {
                    index.search(q, 10).unwrap();
                }
                queries.len()
            })
            .sum();
        println!(
            "REF usearch M={m} ef={ef}: recall@10={:.4} p50={:.3}ms p99={:.3}ms qps8={:.0}",
            found as f64 / (10 * queries.len()) as f64,
            ms(lat[lat.len() / 2]),
            ms(lat[lat.len() * 99 / 100]),
            total as f64 / t.elapsed().as_secs_f64()
        );
    }
}

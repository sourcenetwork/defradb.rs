//! usearch M 32 saved to disk, then served by mmap (`view`), for the
//! memory-capped runs.
//!
//! `usearch_view build <base.f32> <bench dir> <index file>` builds and saves it;
//! `usearch_view view <index file> <bench dir> <ef> <warmup.f32>` maps it,
//! runs 500 warm-up queries, then measures the bench dir's queries. The bench dir
//! holds the corpus `dims` and `count`, which both commands follow.

use std::path::Path;
use std::time::Instant;

use rayon::prelude::*;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};
use vector_bench::{hits, read_base, read_f32, read_u64};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let options = |dimensions| IndexOptions {
        dimensions,
        metric: MetricKind::Cos,
        quantization: ScalarKind::F32,
        connectivity: 32,
        expansion_add: 200,
        expansion_search: 128,
        multi: false,
    };
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [_, "build", base, bench, path] => {
            let (dims, base) = read_base(Path::new(base), Path::new(bench));
            let index = Index::new(&options(dims)).unwrap();
            index.reserve(base.len()).unwrap();
            base.par_iter()
                .enumerate()
                .for_each(|(i, v)| index.add(i as u64 + 1, v).unwrap());
            index.save(path).unwrap();
            println!("SAVED {} MiB", std::fs::metadata(path).unwrap().len() >> 20);
        }
        [_, "view", path, bench, ef, warmup] => {
            let dims: usize = std::fs::read_to_string(Path::new(bench).join("dims"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let index = Index::new(&options(dims)).unwrap();
            index.view(path).unwrap();
            let ef: usize = ef.parse().unwrap();
            index.change_expansion_search(ef);
            let queries = read_f32(&Path::new(bench).join("queries.f32"), dims);
            let truth = read_u64(&Path::new(bench).join("truth.u64"));
            let t0 = Instant::now();
            for q in read_f32(Path::new(warmup), dims).iter().take(500) {
                index.search(q, 10).unwrap();
            }
            println!("WARMUP 500 in {:.1}s", t0.elapsed().as_secs_f64());
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
            let total: usize = queries
                .par_iter()
                .map(|q| {
                    index.search(q, 10).unwrap();
                    1
                })
                .sum();
            println!(
                "RESULT usearch-view M=32 ef={ef}: recall@10={:.4} p50={:.3}ms p99={:.3}ms qps8={:.0}",
                found as f64 / (10 * queries.len()) as f64,
                ms(lat[lat.len() / 2]),
                ms(lat[lat.len() * 99 / 100]),
                total as f64 / t.elapsed().as_secs_f64()
            );
        }
        _ => panic!("usage: usearch_view build <base.f32> <bench dir> <index> | view <index> <bench dir> <ef> <warmup.f32>"),
    }
}

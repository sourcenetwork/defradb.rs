//! k-means cost and quality on a real corpus.
//!
//! ```text
//! KMEANS_CORPUS_DIR=<dir with base.f32, queries.f32, gt10.u32> \
//!   cargo test --release -p db --test vector -- kmeans_real_corpus --ignored --nocapture
//! ```
//!
//! `KMEANS_N` (default 87,381, what the default 128 MiB sample holds at 384
//! dimensions) and `KMEANS_K` (default 1,385, `4*sqrt(120k)`) size the fit.

use std::time::Instant;

use db::index::vector::quantize::{Centroids, Clusterer, KMeans};
use defra_core::vector::squared_euclidean;

const DIMENSIONS: usize = 384;
const K: usize = 10;
const QUERIES: usize = 300;
const NPROBE: usize = 8;

fn read_f32(path: &str) -> Vec<f32> {
    std::fs::read(path)
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Recall@10 of an IVF-Flat scan over `centroids`: probe the `NPROBE`
/// nearest lists, score their members exactly.
fn ivf_recall(base: &[f32], queries: &[f32], truth: &[u32], centroids: &Centroids) -> f64 {
    let n = base.len() / DIMENSIONS;
    let point =
        |s: &[f32], i: usize| -> Vec<f32> { s[i * DIMENSIONS..(i + 1) * DIMENSIONS].to_vec() };
    let mut lists: Vec<Vec<usize>> = vec![Vec::new(); centroids.k];
    for i in 0..n {
        lists[centroids.nearest(&point(base, i)).0].push(i);
    }

    let mut hit = 0usize;
    for q in 0..QUERIES {
        let query = point(queries, q);
        let mut ranked: Vec<(usize, f64)> = (0..centroids.k)
            .map(|c| (c, squared_euclidean(&query, centroids.get(c))))
            .collect();
        ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut scored: Vec<(usize, f64)> = ranked[..NPROBE]
            .iter()
            .flat_map(|(c, _)| lists[*c].iter())
            .map(|&i| (i, squared_euclidean(&query, &point(base, i))))
            .collect();
        scored.sort_by(|a, b| a.1.total_cmp(&b.1));
        let want = &truth[q * K..(q + 1) * K];
        hit += scored
            .iter()
            .take(K)
            .filter(|(i, _)| want.contains(&(*i as u32)))
            .count();
    }
    hit as f64 / (K * QUERIES) as f64
}

#[test]
#[ignore = "measurement; needs KMEANS_CORPUS_DIR"]
fn kmeans_real_corpus() {
    let dir = std::env::var("KMEANS_CORPUS_DIR").expect("KMEANS_CORPUS_DIR");
    let base = read_f32(&format!("{dir}/base.f32"));
    let queries = read_f32(&format!("{dir}/queries.f32"));
    let truth: Vec<u32> = std::fs::read(format!("{dir}/gt10.u32"))
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect();

    let n = env_usize("KMEANS_N", 87_381).min(base.len() / DIMENSIONS);
    let k = env_usize("KMEANS_K", 1_385);
    let sample = &base[..n * DIMENSIONS];

    let started = Instant::now();
    let (centroids, fit) = KMeans::new(7).fit(sample, DIMENSIONS, k);
    println!(
        "fit n={n} k={k}: {:.1}s, {} iterations, inertia {:.5}",
        started.elapsed().as_secs_f64(),
        fit.iterations,
        fit.inertia
    );

    let started = Instant::now();
    let lists: usize = base
        .chunks(DIMENSIONS)
        .map(|v| centroids.nearest(v).0)
        .sum();
    println!(
        "assign {} vectors one at a time: {:.1}s (checksum {lists})",
        base.len() / DIMENSIONS,
        started.elapsed().as_secs_f64()
    );

    if std::env::var("KMEANS_RECALL").is_ok() {
        let started = Instant::now();
        let recall = ivf_recall(&base, &queries, &truth, &centroids);
        println!(
            "ivf-flat nprobe={NPROBE} recall@10 {recall:.4} ({:.1}s)",
            started.elapsed().as_secs_f64()
        );
    }
}

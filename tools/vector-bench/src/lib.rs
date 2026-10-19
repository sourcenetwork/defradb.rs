//! Shared readers for the raw little-endian files `vector_disk_cost` uses.

use std::path::Path;

pub fn read_f32(path: &Path, dims: usize) -> Vec<Vec<f32>> {
    let bytes = std::fs::read(path).unwrap();
    let floats: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    floats.chunks_exact(dims).map(<[f32]>::to_vec).collect()
}

pub fn read_u64(path: &Path) -> Vec<u64> {
    std::fs::read(path)
        .unwrap()
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| u64::from_le_bytes(*b))
        .collect()
}

/// Peak and current resident set from `/proc/self/status`.
pub fn rss() -> String {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let get = |key: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .unwrap()
            .trim()
            .to_string()
    };
    format!("rss={} hwm={}", get("VmRSS:"), get("VmHWM:"))
}

/// Recall@10 hits of `got` against the exact top 10 of query `i`.
pub fn hits(truth: &[u64], i: usize, got: &[u64]) -> usize {
    let want = &truth[i * 10..i * 10 + 10];
    got.iter().filter(|k| want.contains(k)).count()
}

//! Merging a Json field's block.
//!
//! ```text
//! cargo bench -p benches --bench json_merge
//! ```
//!
//! A field of `KEYS` keys, each written by one block of a linear history, so
//! its frontier holds one entry per key. `fast_path` is a block whose parents
//! are the field's heads: one key set over the frontier, every entry in its
//! past. `concurrent` is the same edit from a block that has not seen the
//! last write, so the merge asks of each entry whether it is in the block's
//! past. `read` materializes the field, and `conflicts` lists its losers;
//! `replay` rebuilds the frontier from the whole DAG, as a time-travel read
//! does.

use std::collections::BTreeSet;
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use defra_core::json_crdt::{self, Frontier, JsonEdit, JsonOp, ReplayBlock};
use serde_json::json;

mod common;

const KEYS: [usize; 4] = [10, 100, 1_000, 10_000];

/// `keys` blocks, each setting one key, and the frontier they leave.
fn history(keys: usize) -> (Vec<ReplayBlock>, Frontier) {
    let mut blocks: Vec<ReplayBlock> = Vec::with_capacity(keys);
    let mut frontier = Frontier::default();
    for n in 0..keys {
        let ops = vec![JsonOp {
            path: vec![format!("k{n}")],
            edit: JsonEdit::Set(json!(n)),
        }];
        let data = json_crdt::encode_ops(&ops).expect("ops encode");
        let parents: Vec<Vec<u8>> = blocks.last().map(|b| b.cid.clone()).into_iter().collect();
        let priority = n as u64 + 1;
        let id = json_crdt::block_id(priority, &parents, &data);
        frontier.compose(frontier.local_delta(&ops, &id, priority));
        blocks.push(ReplayBlock {
            cid: id,
            parents,
            priority,
            data,
        });
    }
    (blocks, frontier)
}

fn merge(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_merge");
    for keys in KEYS {
        let (blocks, frontier) = history(keys);
        let edit = vec![JsonOp {
            path: vec!["k0".into()],
            edit: JsonEdit::Set(json!("edited")),
        }];
        let id = vec![0xff; 32];
        group.bench_with_input(BenchmarkId::new("fast_path", keys), &frontier, |b, f| {
            b.iter(|| {
                let mut f = f.clone();
                f.compose(f.local_delta(black_box(&edit), &id, keys as u64 + 1));
                black_box(f)
            })
        });
        let last = blocks.last().map(|b| b.cid.clone()).unwrap_or_default();
        let past: BTreeSet<Vec<u8>> = blocks
            .iter()
            .map(|b| b.cid.clone())
            .filter(|cid| *cid != last)
            .collect();
        group.bench_with_input(BenchmarkId::new("concurrent", keys), &frontier, |b, f| {
            b.iter(|| {
                let mut f = f.clone();
                f.compose(f.delta(black_box(&edit), &id, keys as u64, |e| past.contains(e)));
                black_box(f)
            })
        });
        group.bench_with_input(BenchmarkId::new("read", keys), &frontier, |b, f| {
            b.iter(|| black_box(f.materialize()))
        });
        group.bench_with_input(BenchmarkId::new("conflicts", keys), &frontier, |b, f| {
            b.iter(|| black_box(f.conflicts()))
        });
        group.bench_function(BenchmarkId::new("replay", keys), |b| {
            b.iter(|| {
                let blocks = blocks
                    .iter()
                    .map(|b| ReplayBlock {
                        cid: b.cid.clone(),
                        parents: b.parents.clone(),
                        priority: b.priority,
                        data: b.data.clone(),
                    })
                    .collect();
                black_box(json_crdt::replay(blocks).expect("a whole history"))
            })
        });
    }
    group.finish();
}

criterion_group!(benches, merge);
criterion_main!(benches);

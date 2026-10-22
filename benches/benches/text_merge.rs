//! Merging a Text field's block.
//!
//! ```text
//! cargo bench -p benches --bench text_merge
//! ```
//!
//! A block whose parents are the field's heads applies to the stored text as
//! plain edits (the fast path). Any other block replays from the field's last
//! critical block (`critical`); `replay` is the whole history, which a merge
//! falls back to. Swept by history depth: one writer's edits, each a
//! five-character insert or a three-character delete at a random spot, then
//! one block concurrent with the last edit.

use std::hint::black_box;

use crdt::fugue::{merge_text, replay, TextBlock};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use defra_core::TextOp;

mod common;

const DEPTHS: [usize; 4] = [10, 100, 1_000, 10_000];

/// A CID-like ID: hashed, so sibling order is unrelated to write order.
fn id(n: usize) -> Vec<u8> {
    let mut x = (n as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut id = vec![0x01, 0x71, 0x12, 0x20];
    for _ in 0..4 {
        x ^= x >> 31;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        id.extend_from_slice(&x.to_be_bytes());
    }
    id
}

/// `depth` linear blocks and the text they leave.
fn history(depth: usize) -> (Vec<TextBlock>, String) {
    let mut seed = depth as u64;
    let mut below = |n: u64| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % n.max(1)
    };
    let mut blocks = Vec::with_capacity(depth + 1);
    let mut text = String::new();
    for n in 0..depth {
        let len = text.chars().count() as u64;
        let op = if len > 10 && below(4) == 0 {
            TextOp::Delete {
                pos: below(len - 3),
                len: 3,
            }
        } else {
            TextOp::Insert {
                pos: below(len + 1),
                text: "hello".into(),
            }
        };
        text = TextOp::apply_all(&text, std::slice::from_ref(&op));
        blocks.push(TextBlock {
            id: id(n),
            parents: n.checked_sub(1).map(id).into_iter().collect(),
            ops: vec![op],
        });
    }
    (blocks, text)
}

fn merge(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_merge");
    for depth in DEPTHS {
        let (mut blocks, text) = history(depth);
        let edit = vec![TextOp::Insert {
            pos: text.chars().count() as u64 / 2,
            text: "world".into(),
        }];
        blocks.push(TextBlock {
            id: id(depth + 1),
            parents: depth.checked_sub(2).map(id).into_iter().collect(),
            ops: edit.clone(),
        });
        group.bench_with_input(BenchmarkId::new("fast_path", depth), &text, |b, text| {
            b.iter(|| black_box(TextOp::apply_all(black_box(text), &edit)))
        });
        group.bench_with_input(BenchmarkId::new("replay", depth), &blocks, |b, blocks| {
            b.iter(|| black_box(replay(black_box(blocks)).expect("a whole history")))
        });
        // The new block's parent is the last critical block: the replay runs
        // the edit it is concurrent with, and itself.
        let base = replay(&blocks[..depth - 1])
            .expect("a whole history")
            .chars()
            .count();
        let after = blocks[depth - 1..].to_vec();
        let new = id(depth + 1);
        group.bench_with_input(BenchmarkId::new("critical", depth), &after, |b, after| {
            b.iter(|| {
                black_box(
                    merge_text(black_box(&text), base, black_box(after), &new)
                        .expect("blocks after a critical block")
                        .expect("the stored text fits them"),
                )
            })
        });
    }
    group.finish();
}

criterion_group!(benches, merge);
criterion_main!(benches);

//! Insert, update, delete, build and merge across every vector index kind.
//!
//! ```text
//! cargo bench -p db --bench vector_mutations
//! ```
//!
//! An index is rebuilt for every measured batch rather than mutated in place,
//! because a graph's insert cost depends on how many nodes are already there.
//! The corpus size is therefore part of what is measured, not an accident of
//! iteration order.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use db::index::vector::store::NodeId;
use std::hint::black_box;

mod common;

use common::vector::{Corpus, Index, Kind, ALL_KINDS, SEED};

/// Every kind in every state a mutation can hit. A batch-built kind behaves
/// like a flat scan until it is built, so measuring only that state would
/// report a number no production query ever sees.
fn states() -> Vec<(Kind, bool, String)> {
    let mut out = Vec::new();
    for kind in ALL_KINDS {
        out.push((kind, false, kind.name().to_string()));
        if kind.is_batch_built() {
            out.push((kind, true, format!("{}[built]", kind.name())));
        }
    }
    out
}

const DIMENSIONS: usize = 128;
const CORPUS: usize = 2_000;
const BATCH: usize = 100;

/// Inserting into an index that already holds `CORPUS` vectors, which is what
/// a write to a populated collection costs.
fn insert(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut corpus = Corpus::new(SEED);
    let existing = corpus.clustered(CORPUS, DIMENSIONS, 32, 0.2);
    let incoming = corpus.clustered(BATCH, DIMENSIONS, 32, 0.2);

    let mut group = c.benchmark_group("insert");
    group.throughput(Throughput::Elements(BATCH as u64));

    for (kind, built, label) in states() {
        let fixture = rt.block_on(Index::filled(kind, &existing, built));
        group.bench_with_input(BenchmarkId::from_parameter(&label), &kind, |b, _kind| {
            b.to_async(&rt).iter_batched(
                || fixture.clone(),
                |mut index| {
                    let incoming = &incoming;
                    async move {
                        for (i, vector) in incoming.iter().enumerate() {
                            index
                                .insert(NodeId((CORPUS + i + 1) as u64), black_box(vector))
                                .await;
                        }
                        index
                    }
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// An update is a re-insert under an id that already exists: the node's vector
/// is replaced and its own links rebuilt, while links pointing *at* it stay
/// valid because the id is what they name.
fn update(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut corpus = Corpus::new(SEED ^ 0x11);
    let existing = corpus.clustered(CORPUS, DIMENSIONS, 32, 0.2);
    let replacements = corpus.clustered(BATCH, DIMENSIONS, 32, 0.2);

    let mut group = c.benchmark_group("update");
    group.throughput(Throughput::Elements(BATCH as u64));

    for (kind, built, label) in states() {
        let fixture = rt.block_on(Index::filled(kind, &existing, built));
        group.bench_with_input(BenchmarkId::from_parameter(&label), &kind, |b, _kind| {
            b.to_async(&rt).iter_batched(
                || fixture.clone(),
                |mut index| {
                    let replacements = &replacements;
                    async move {
                        for (i, vector) in replacements.iter().enumerate() {
                            index.insert(NodeId(i as u64 + 1), black_box(vector)).await;
                        }
                        index
                    }
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// Deletes are tombstones on every kind, so this measures the write and the
/// read that precedes it, not a graph repair.
fn delete(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut corpus = Corpus::new(SEED ^ 0x22);
    let existing = corpus.clustered(CORPUS, DIMENSIONS, 32, 0.2);

    let mut group = c.benchmark_group("delete");
    group.throughput(Throughput::Elements(BATCH as u64));

    for (kind, built, label) in states() {
        let fixture = rt.block_on(Index::filled(kind, &existing, built));
        group.bench_with_input(BenchmarkId::from_parameter(&label), &kind, |b, _kind| {
            b.to_async(&rt).iter_batched(
                || fixture.clone(),
                |mut index| async move {
                    for i in 0..BATCH {
                        black_box(index.delete(NodeId(i as u64 + 1)).await);
                    }
                    index
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// Building the whole index from scratch: what creating an index on a
/// populated collection costs.
fn bulk_load(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut corpus = Corpus::new(SEED ^ 0x33);
    let vectors = corpus.clustered(CORPUS, DIMENSIONS, 32, 0.2);

    let mut group = c.benchmark_group("bulk_load");
    group.sample_size(10);
    group.throughput(Throughput::Elements(CORPUS as u64));

    for kind in ALL_KINDS {
        group.bench_with_input(
            BenchmarkId::from_parameter(kind.name()),
            &kind,
            |b, &kind| {
                b.to_async(&rt)
                    .iter(|| async { black_box(Index::filled(kind, &vectors, false).await) });
            },
        );
    }
    group.finish();
}

/// The train-and-build pass the batch-built kinds run once the corpus is large
/// enough. The graph kinds have no equivalent, so they are absent rather than
/// reported as zero.
fn build(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut corpus = Corpus::new(SEED ^ 0x44);
    let vectors = corpus.clustered(CORPUS, DIMENSIONS, 32, 0.2);

    let mut group = c.benchmark_group("build");
    group.sample_size(10);
    group.throughput(Throughput::Elements(CORPUS as u64));

    for kind in ALL_KINDS
        .iter()
        .copied()
        .filter(|kind| kind.is_batch_built())
    {
        let fixture = rt.block_on(Index::filled(kind, &vectors, false));
        group.bench_with_input(
            BenchmarkId::from_parameter(kind.name()),
            &kind,
            |b, _kind| {
                b.to_async(&rt).iter_batched(
                    || fixture.clone(),
                    |mut index| async move {
                        index.build().await;
                        index
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

/// Folding a batch of pending inserts into a built graph: the streaming merge
/// the kinds that defer graph work to a background sweep run there, in
/// place of the work an insert into a graph kind does inline.
fn merge(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut corpus = Corpus::new(SEED ^ 0x4D);
    let existing = corpus.clustered(CORPUS, DIMENSIONS, 32, 0.2);
    let incoming = corpus.clustered(BATCH, DIMENSIONS, 32, 0.2);

    let mut group = c.benchmark_group("merge");
    group.sample_size(10);
    group.throughput(Throughput::Elements(BATCH as u64));

    for kind in ALL_KINDS
        .iter()
        .copied()
        .filter(|kind| kind.merges_pending())
    {
        let fixture = rt.block_on(async {
            let mut index = Index::filled(kind, &existing, true).await;
            for (i, vector) in incoming.iter().enumerate() {
                index.insert(NodeId((CORPUS + i + 1) as u64), vector).await;
            }
            index
        });
        group.bench_with_input(
            BenchmarkId::from_parameter(kind.name()),
            &kind,
            |b, _kind| {
                b.to_async(&rt).iter_batched(
                    || fixture.clone(),
                    |mut index| async move {
                        index.build().await;
                        index
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

/// One insert committed through `VectorIndex` on a regolith store, the way a
/// collection writes it: a fresh engine per transaction, so the codebook
/// decode and the encode are both part of it.
fn insert_through_index(c: &mut Criterion) {
    use std::sync::atomic::{AtomicU64, Ordering};
    use storage::corekv::Store;
    use storage::index::CollectionIndex;

    let rt = common::owned_runtime();
    let dimensions = 768;
    let (store, index) = rt.block_on(common::vector::trained_ivfpq(dimensions));
    let mut corpus = Corpus::new(SEED ^ 0x1FB);
    let vectors: Vec<_> = (0..64).map(|_| corpus.vector(dimensions)).collect();
    let next = AtomicU64::new(1 << 32);

    let mut group = c.benchmark_group("insert_through_index");
    group.bench_function(BenchmarkId::new("ivfpq[built]", dimensions), |b| {
        b.to_async(&rt).iter(|| async {
            let id = next.fetch_add(1, Ordering::Relaxed);
            let value = vec![document::NormalValue::Float32Array(
                vectors[id as usize % vectors.len()].clone(),
            )];
            let mut txn = store.new_txn(false).await.expect("a transaction");
            index
                .save(&mut txn, id, black_box(&value))
                .await
                .expect("save");
            txn.commit().await.expect("commit");
        });
    });
    group.finish();
}

criterion_group!(
    mutations,
    insert,
    update,
    delete,
    bulk_load,
    build,
    merge,
    insert_through_index
);
criterion_main!(mutations);

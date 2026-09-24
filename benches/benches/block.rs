use std::str::FromStr;

use cid::Cid;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use defra_core::block::generate_cid_from_bytes;
use defra_core::{
    Block, CollectionDeltaPayload, CompositeDeltaPayload, CounterDeltaPayload, CrdtDelta, DAGLink,
    LwwDeltaPayload,
};
use std::hint::black_box;

#[path = "../../crates/defra-core/tests/fixtures/go_blocks.rs"]
mod go_blocks;
use go_blocks::*;

/// Every golden vector as `(name, go_bytes, go_cid)`.
const GO_VECTORS: &[(&str, &[u8], &str)] = &[
    ("lww_simple", GO_LWW_SIMPLE_BYTES, GO_LWW_SIMPLE_CID),
    (
        "lww_high_priority",
        GO_LWW_HIGH_PRIORITY_BYTES,
        GO_LWW_HIGH_PRIORITY_CID,
    ),
    ("lww_deletion", GO_LWW_DELETION_BYTES, GO_LWW_DELETION_CID),
    ("counter", GO_COUNTER_BYTES, GO_COUNTER_CID),
    (
        "composite_active",
        GO_COMPOSITE_ACTIVE_BYTES,
        GO_COMPOSITE_ACTIVE_CID,
    ),
    (
        "composite_deleted",
        GO_COMPOSITE_DELETED_BYTES,
        GO_COMPOSITE_DELETED_CID,
    ),
    ("collection", GO_COLLECTION_BYTES, GO_COLLECTION_CID),
];

/// Correctness gate: every golden vector must decode, re-encode byte-identically,
/// and hash to the CID Go computed. Runs before any timing so a parity break is a
/// hard failure rather than a silently fast number.
fn assert_go_parity() {
    for (name, go_bytes, go_cid) in GO_VECTORS {
        let block = Block::from_dag_cbor(go_bytes)
            .unwrap_or_else(|e| panic!("GO PARITY BREAK [{name}]: decode failed: {e}"));

        let rust_bytes = block
            .to_dag_cbor()
            .unwrap_or_else(|e| panic!("GO PARITY BREAK [{name}]: encode failed: {e}"));
        assert_eq!(
            rust_bytes.as_slice(),
            *go_bytes,
            "GO PARITY BREAK [{name}]: re-encoded bytes differ from Go bytes"
        );

        let expected = Cid::from_str(go_cid).unwrap();
        let from_bytes = generate_cid_from_bytes(go_bytes).unwrap();
        assert_eq!(
            from_bytes, expected,
            "GO PARITY BREAK [{name}]: generate_cid_from_bytes CID differs from Go CID"
        );
        assert_eq!(
            block.generate_cid().unwrap(),
            expected,
            "GO PARITY BREAK [{name}]: Block::generate_cid CID differs from Go CID"
        );
    }
}

// ============================================================================
// Synthetic blocks (parameterized by link count and payload size)
// ============================================================================

fn links(count: usize) -> Vec<DAGLink> {
    (0..count)
        .map(|index| {
            DAGLink::new(
                format!("field_{index}"),
                generate_cid_from_bytes(format!("link-{index}").as_bytes()).unwrap(),
            )
        })
        .collect()
}

fn lww_block(link_count: usize, payload_len: usize) -> Block {
    Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".to_string(),
            priority: 1,
            schema_version_id: "schema1".to_string(),
            data: vec![0xABu8; payload_len],
        }),
        vec![],
        links(link_count),
    )
}

fn composite_block_with_links() -> Block {
    Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: "schema1".to_string(),
            priority: 7,
            status: 1,
        }),
        vec![],
        links(5),
    )
}

const LINK_COUNTS: [usize; 3] = [0, 5, 20];
const PAYLOAD_SIZES: [usize; 2] = [16, 1024];

// ============================================================================
// Benches
// ============================================================================

fn bench_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("cbor");

    let lww = lww_block(0, 4);
    group.bench_function(BenchmarkId::from_parameter("encode_lww_small"), |b| {
        b.iter(|| black_box(lww.to_dag_cbor().unwrap()));
    });

    let composite = composite_block_with_links();
    group.bench_function(
        BenchmarkId::from_parameter("encode_composite_5_links"),
        |b| {
            b.iter(|| black_box(composite.to_dag_cbor().unwrap()));
        },
    );

    let lww_bytes = lww.to_dag_cbor().unwrap();
    group.bench_function(BenchmarkId::from_parameter("decode_lww"), |b| {
        b.iter(|| black_box(Block::from_dag_cbor(black_box(&lww_bytes)).unwrap()));
    });

    group.bench_function(BenchmarkId::from_parameter("generate_cid"), |b| {
        b.iter(|| black_box(lww.generate_cid().unwrap()));
    });

    group.bench_function(BenchmarkId::from_parameter("encode_and_cid"), |b| {
        b.iter(|| {
            let bytes = lww.to_dag_cbor().unwrap();
            black_box(generate_cid_from_bytes(black_box(&bytes)).unwrap());
        });
    });

    group.finish();
}

/// Benches driven by the byte-exact Go vectors above.
fn bench_go_vectors(c: &mut Criterion) {
    assert_go_parity();

    let mut group = c.benchmark_group("cbor_go_vectors");

    for (name, go_bytes, go_cid) in GO_VECTORS {
        let expected_cid = Cid::from_str(go_cid).unwrap();
        group.throughput(Throughput::Bytes(go_bytes.len() as u64));

        group.bench_function(BenchmarkId::new("decode_go", name), |b| {
            b.iter(|| black_box(Block::from_dag_cbor(black_box(go_bytes)).unwrap()));
        });

        group.bench_function(BenchmarkId::new("reencode_go", name), |b| {
            // `_ref`: criterion drops the routine's return value outside the measurement
            // but not its argument, so a by-value `Block` would have its teardown timed
            // alongside the encode.
            b.iter_batched_ref(
                || Block::from_dag_cbor(go_bytes).unwrap(),
                |block| black_box(block.to_dag_cbor().unwrap()),
                criterion::BatchSize::SmallInput,
            );
        });

        group.bench_function(BenchmarkId::new("decode_reencode_go", name), |b| {
            b.iter(|| {
                let block = Block::from_dag_cbor(black_box(go_bytes)).unwrap();
                black_box(block.to_dag_cbor().unwrap())
            });
        });

        group.bench_function(BenchmarkId::new("cid_from_go_bytes", name), |b| {
            b.iter(|| {
                let cid = generate_cid_from_bytes(black_box(go_bytes)).unwrap();
                debug_assert_eq!(cid, expected_cid);
                black_box(cid)
            });
        });
    }

    group.finish();
}

/// Encode/decode/CID curves over link count and payload size, matching the
/// shape of the equivalent Go block benchmarks.
fn bench_block_shape(c: &mut Criterion) {
    let mut group = c.benchmark_group("cbor_shape");

    for link_count in LINK_COUNTS {
        for payload in PAYLOAD_SIZES {
            let block = lww_block(link_count, payload);
            let bytes = block.to_dag_cbor().unwrap();
            let label = format!("links{link_count}_payload{payload}");
            group.throughput(Throughput::Bytes(bytes.len() as u64));

            group.bench_function(BenchmarkId::new("encode", &label), |b| {
                b.iter(|| black_box(block.to_dag_cbor().unwrap()));
            });

            group.bench_function(BenchmarkId::new("decode", &label), |b| {
                b.iter(|| black_box(Block::from_dag_cbor(black_box(&bytes)).unwrap()));
            });

            group.bench_function(BenchmarkId::new("cid", &label), |b| {
                b.iter(|| black_box(generate_cid_from_bytes(black_box(&bytes)).unwrap()));
            });

            group.bench_function(BenchmarkId::new("encode_and_cid", &label), |b| {
                b.iter(|| {
                    let encoded = block.to_dag_cbor().unwrap();
                    black_box(generate_cid_from_bytes(black_box(&encoded)).unwrap())
                });
            });
        }
    }

    group.finish();
}

/// Per-delta-variant encode cost at a fixed shape, so variant overhead is
/// separable from payload/link overhead.
fn bench_delta_variants(c: &mut Criterion) {
    let mut group = c.benchmark_group("cbor_variants");

    let variants: Vec<(&str, Block)> = vec![
        (
            "lww",
            Block::new(
                CrdtDelta::Lww(LwwDeltaPayload {
                    field_name: "name".to_string(),
                    priority: 1,
                    schema_version_id: "schema1".to_string(),
                    data: b"John".to_vec(),
                }),
                vec![],
                vec![],
            ),
        ),
        (
            "counter",
            Block::new(
                CrdtDelta::Counter(CounterDeltaPayload {
                    field_name: "count".to_string(),
                    priority: 1,
                    nonce: 12345,
                    schema_version_id: "schema1".to_string(),
                    data: vec![0x0A],
                }),
                vec![],
                vec![],
            ),
        ),
        (
            "composite",
            Block::new(
                CrdtDelta::Composite(CompositeDeltaPayload {
                    schema_version_id: "schema1".to_string(),
                    priority: 1,
                    status: 1,
                }),
                vec![],
                vec![],
            ),
        ),
        (
            "collection",
            Block::new(
                CrdtDelta::Collection(CollectionDeltaPayload {
                    schema_version_id: "schema1".to_string(),
                    priority: 1,
                }),
                vec![],
                vec![],
            ),
        ),
    ];

    for (name, block) in &variants {
        let bytes = block.to_dag_cbor().unwrap();
        group.throughput(Throughput::Bytes(bytes.len() as u64));

        group.bench_function(BenchmarkId::new("encode", name), |b| {
            b.iter(|| black_box(block.to_dag_cbor().unwrap()));
        });

        group.bench_function(BenchmarkId::new("decode", name), |b| {
            b.iter(|| black_box(Block::from_dag_cbor(black_box(&bytes)).unwrap()));
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_block,
    bench_go_vectors,
    bench_block_shape,
    bench_delta_variants
);
criterion_main!(benches);

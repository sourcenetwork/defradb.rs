//! The replication wire: what a pushed block costs to frame.
//!
//! ```text
//! cargo bench -p benches --bench p2p_codec
//! ```
//!
//! Every replicated document crosses this boundary twice, once serialized on
//! the sender and once decoded on the receiver.
//!
//! Swept by block size, because these are byte-rate operations and a figure
//! quoted for a small block says nothing about a large one.

use std::hint::black_box;

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use p2p::message::{PushLogBroadcast, PushLogRequest};

mod common;

const BLOCK_SIZES: [usize; 4] = [256, 4 * 1024, 64 * 1024, 512 * 1024];
const CID_BYTES: [u8; 36] = [0x01; 36];

fn request(block_size: usize) -> PushLogRequest {
    PushLogRequest::new(
        "bae-8a1f9c2d-4e5b-4c3a-9d7e-1f2a3b4c5d6e".into(),
        Bytes::from_static(&CID_BYTES),
        "collection-1".into(),
        "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH".into(),
        Bytes::from(
            (0..block_size)
                .map(|i| (i % 251) as u8)
                .collect::<Vec<u8>>(),
        ),
    )
}

fn cbor(c: &mut Criterion) {
    let mut group = c.benchmark_group("p2p_cbor");
    for size in BLOCK_SIZES {
        let message = request(size);
        let encoded = defra_core::cbor::to_vec(&message).expect("the message to encode");
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("encode", size), &message, |b, message| {
            b.iter(|| black_box(defra_core::cbor::to_vec(black_box(message)).expect("encode")))
        });
        group.bench_with_input(BenchmarkId::new("decode", size), &encoded, |b, encoded| {
            b.iter(|| {
                black_box(
                    defra_core::cbor::from_slice::<PushLogRequest>(black_box(encoded))
                        .expect("decode"),
                )
            })
        });
    }
    group.finish();
}

/// The gossip path frames the same request differently from the request path,
/// so its cost is measured on its own rather than assumed equal.
fn gossip(c: &mut Criterion) {
    let mut group = c.benchmark_group("p2p_gossip");
    for size in BLOCK_SIZES {
        let broadcast = PushLogBroadcast::from_request(&request(size));
        let payload = broadcast
            .encode_gossip_payload()
            .expect("the payload to encode");
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("encode", size),
            &broadcast,
            |b, broadcast| {
                b.iter(|| {
                    black_box(
                        black_box(broadcast)
                            .encode_gossip_payload()
                            .expect("encode"),
                    )
                })
            },
        );
        group.bench_with_input(BenchmarkId::new("decode", size), &payload, |b, payload| {
            b.iter(|| {
                black_box(
                    PushLogBroadcast::decode_gossip_payload(black_box(payload)).expect("decode"),
                )
            })
        });
    }
    group.finish();
}

criterion_group!(benches, cbor, gossip);
criterion_main!(benches);

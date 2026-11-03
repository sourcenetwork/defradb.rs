//! Shared state under concurrent access.
//!
//! ```text
//! cargo bench -p benches --bench contention
//! ```
//!
//! These rows put several threads on one shared structure at once and report
//! aggregate throughput, exposing contention in the collection cache and
//! event-bus subscriber map. Actual document writes are covered by
//! `write_baseline` and the document-write benchmarks.
//!
//! Every structure here is reached through a public API that exists on both
//! sides of the lock-free change, so the same file measures both.

use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use db::DB;
use events::{Bus, ChannelBus, EventName, Message};
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

mod common;

/// Thread counts. 1 is the uncontended baseline every other bench measures;
/// the rest are where a lock starts costing more than the work it guards.
const THREADS: [usize; 5] = [1, 2, 4, 8, 16];

/// Collections in the cache. A realistic node has tens, and the lookup walks
/// a hash map either way, so the count only decides the map's size.
const COLLECTIONS: usize = 32;

fn collection(index: usize) -> CollectionVersion {
    CollectionVersion::new(
        format!("Collection{index}"),
        format!("collection-v{index}"),
        format!("col-{index}"),
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "body", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
}

/// A database whose collection cache holds [`COLLECTIONS`] entries.
fn populated_db() -> Arc<DB<RegolithStore>> {
    let store = Arc::new(RegolithStore::in_memory().expect("an in-memory store"));
    let db = Arc::new(DB::from_arc(store).expect("a database over it"));
    let rt = common::owned_runtime();
    rt.block_on(async {
        for index in 0..COLLECTIONS {
            db.create_collection(collection(index))
                .await
                .expect("the collection to register");
        }
    });
    db
}

/// Run `work` on `threads` threads for `duration`, returning operations per
/// second across all of them.
///
/// Wall time rather than a criterion iteration count: what matters here is
/// aggregate throughput as threads are added, and criterion times one
/// iteration on one thread. Every thread waits for a start gate so none of them
/// measures another's startup.
fn throughput<W>(threads: usize, duration: std::time::Duration, work: W) -> f64
where
    W: Fn(usize) -> u64 + Send + Sync + 'static,
{
    let work = Arc::new(work);
    let ready = Arc::new(AtomicUsize::new(0));
    let go = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let total = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..threads)
        .map(|id| {
            let work = Arc::clone(&work);
            let ready = Arc::clone(&ready);
            let go = Arc::clone(&go);
            let stop = Arc::clone(&stop);
            let total = Arc::clone(&total);
            std::thread::spawn(move || {
                ready.fetch_add(1, Ordering::AcqRel);
                while !go.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                let mut done = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    // A batch between stop checks, so the flag read does not
                    // dominate the operation being measured.
                    for _ in 0..64 {
                        done += work(id);
                    }
                }
                total.fetch_add(done, Ordering::Relaxed);
            })
        })
        .collect();

    while ready.load(Ordering::Acquire) < threads {
        std::thread::yield_now();
    }
    go.store(true, Ordering::Release);
    let start = Instant::now();
    std::thread::sleep(duration);
    stop.store(true, Ordering::Relaxed);
    for handle in handles {
        handle.join().expect("a worker thread panicked");
    }
    let elapsed = start.elapsed().as_secs_f64();
    total.load(Ordering::Relaxed) as f64 / elapsed
}

const MEASURE: std::time::Duration = std::time::Duration::from_millis(200);

/// The collection cache read by every write path, hammered by readers.
///
/// On a lock this is `RwLock::read` on one cache line per lookup; lock-free
/// it is an atomic load plus an epoch pin. The single-threaded rows in
/// `document_write` measure the first thread only.
fn collection_cache(c: &mut Criterion) {
    let db = populated_db();
    let mut group = c.benchmark_group("contention_collection_cache");
    group.sample_size(10);
    for threads in THREADS {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::from_parameter(threads),
            &threads,
            |b, &threads| {
                b.iter_custom(|iters| {
                    let db = Arc::clone(&db);
                    let ops = throughput(threads, MEASURE, move |id| {
                        let name = format!("Collection{}", id % COLLECTIONS);
                        black_box(db.get_collection(&name).expect("a cache read")).is_some() as u64
                    });
                    // Criterion times `iters` iterations; report the time one
                    // operation took at this thread count.
                    std::time::Duration::from_secs_f64(iters as f64 / ops)
                });
            },
        );
    }
    group.finish();
}

/// The event bus subscriber map, read on every publish and written on every
/// subscribe.
fn event_publish(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention_event_publish");
    group.sample_size(10);
    for threads in THREADS {
        let bus = Arc::new(ChannelBus::new());
        // Held for the whole measurement; a dropped subscription would turn
        // this into a measurement of the bus sweeping dead entries.
        let subscriptions: Vec<_> = (0..8).map(|_| bus.subscribe(&[EventName::Merge])).collect();
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::from_parameter(threads),
            &threads,
            |b, &threads| {
                b.iter_custom(|iters| {
                    let bus = Arc::clone(&bus);
                    let ops = throughput(threads, MEASURE, move |_| {
                        bus.publish(black_box(Message::merge()));
                        1
                    });
                    std::time::Duration::from_secs_f64(iters as f64 / ops)
                });
            },
        );
        drop(subscriptions);
    }
    group.finish();
}

criterion_group!(benches, collection_cache, event_publish);
criterion_main!(benches);

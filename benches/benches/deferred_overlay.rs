//! An indexed query on a counter index that still owes documents an update.
//!
//! ```text
//! cargo bench -p benches --bench deferred_overlay
//! ```
//!
//! A counter increment is a blind write, so its index entry moves later, in a
//! drain. Until then a read through the index checks the owed documents live
//! (#2918). Nothing drains here, so each row times a query over `OWED`
//! distinct owed documents in a corpus of `CORPUS`.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use db::DB;
use document::{Document, FieldOp, NormalValue};
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use rapidhash::{HashSetExt, RapidHashSet};
use storage::RegolithStore;

mod common;

const COLLECTION: &str = "Tally";
const CORPUS: usize = 6_000;
const OWED: [usize; 3] = [0, 1_000, 5_000];

const COLLECTION_SDL: &str = r#"type Tally {
    name: String
    count: Int @crdt(type: "pcounter") @index(name: "by_count")
}"#;

/// `CORPUS` documents counting `0..100`, then one increment on each of the
/// first `owed`, left undrained.
async fn fixture(owed: usize) -> impl QueryExecutor {
    let db = Arc::new(
        DB::new(RegolithStore::in_memory().expect("an in-memory store")).expect("a database"),
    );
    db.add_collection(&defra_core::RequestContext::ambient(), COLLECTION_SDL)
        .await
        .expect("the collection to register");
    let mutator = Arc::new(db::write::autocommit::AutoCommitMutator::new(db.clone()));
    let schema = db.get_collection(COLLECTION).unwrap().unwrap().schema_arc();
    let mut ids = Vec::with_capacity(CORPUS);
    for seq in 0..CORPUS {
        let mut doc = Document::new(schema.clone());
        doc.set("name", NormalValue::String(format!("tally-{seq}")))
            .unwrap();
        doc.set("count", NormalValue::Int((seq % 100) as i64))
            .unwrap();
        ids.push(
            mutator
                .create(COLLECTION, doc)
                .await
                .expect("the seed create to succeed")
                .doc_id,
        );
    }
    let mut changed = RapidHashSet::new();
    changed.insert("count".to_string());
    for id in ids.iter().take(owed) {
        let mut doc = Document::new(schema.clone());
        doc.set_id(id.clone());
        doc.edit("count", FieldOp::Increment(NormalValue::Int(1)))
            .unwrap();
        mutator
            .update(COLLECTION, doc, changed.clone())
            .await
            .expect("the increment to succeed");
    }
    QueryRunner::with_provider(
        db::LensedAutoCommitFetcher::new(db.clone()),
        db::DbCollectionProvider::new_arc(db.clone()),
    )
    .with_mutator(mutator as Arc<dyn DocMutator>)
}

fn owed_overlay(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut group = c.benchmark_group("deferred_overlay");
    group.sample_size(20);
    let queries = [
        (
            "filter_eq",
            "query { Tally(filter: {count: {_eq: 42}}) { name } }",
        ),
        (
            "filter_range",
            "query { Tally(filter: {count: {_gt: 40, _lt: 43}}) { name } }",
        ),
    ];
    for owed in OWED {
        let runner = rt.block_on(fixture(owed));
        for (name, query) in queries {
            // Each increment moves one document in and one out of both filters.
            let expected = if name == "filter_eq" { 60 } else { 120 };
            let response = rt.block_on(runner.execute(QueryRequest::new(query)));
            assert!(response.errors.is_empty(), "{:?}", response.errors);
            let rows = response
                .data
                .as_ref()
                .and_then(|data| data["Tally"].as_array())
                .map(Vec::len);
            assert_eq!(
                rows,
                Some(expected),
                "the overlay must show the owed values"
            );
            group.bench_with_input(BenchmarkId::new(name, owed), &query, |b, query| {
                b.iter(|| {
                    let response =
                        rt.block_on(runner.execute(QueryRequest::new(black_box(*query))));
                    assert!(response.errors.is_empty(), "{:?}", response.errors);
                    black_box(response)
                })
            });
        }
    }
    group.finish();
}

criterion_group!(benches, owed_overlay);
criterion_main!(benches);

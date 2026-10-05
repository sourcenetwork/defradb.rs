//! Introspection cost against the built-schema cache.
//!
//! ```text
//! cargo bench -p benches --bench introspection
//! ```
//!
//! Sequential `__type` queries through the query runner, one row per cache
//! mode:
//!
//! - `off`: every query pays a full async_graphql dynamic schema build over
//!   every collection.
//! - `on`: the provider proves its epoch, so a hit costs an atomic epoch
//!   load, a mutex-guarded slot read and an `Arc` clone; no collection is
//!   materialized at all.
//!
//! Parameterized by collection count, because build cost grows with it: 17
//! and 100 bracket a small node and a schema-heavy one.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use document::Document;
use query::error::Result;
use query::{DocFetcher, QueryExecutor, QueryRequest, QueryRunner};
use schema::{CollectionVersion, FieldDescription, FieldKind};

/// Introspection never touches documents, so the fetcher can be inert.
struct NoDocs;

#[async_trait::async_trait]
impl DocFetcher for NoDocs {
    async fn get_all(&self, _collection_name: &str) -> Result<Vec<Document>> {
        Ok(Vec::new())
    }

    async fn stream_by_doc_short_ids(
        &self,
        _collection_name: &str,
        _doc_short_ids: &[u64],
        _show_deleted: bool,
    ) -> Result<Box<dyn query::doc_stream::DocStream>> {
        Ok(Box::new(query::doc_stream::VecStream::new(Vec::new())))
    }

    async fn stream_all_with_deleted(
        &self,
        _collection_name: &str,
        _show_deleted: bool,
    ) -> Result<Box<dyn query::doc_stream::DocStream>> {
        Ok(Box::new(query::doc_stream::VecStream::new(Vec::new())))
    }

    async fn get_by_ids(
        &self,
        _collection_name: &str,
        _doc_ids: &[String],
    ) -> Result<query::FetchByIdsResult> {
        Ok(query::FetchByIdsResult::all_found(Vec::new()))
    }

    async fn get_by_field_value(
        &self,
        _collection_name: &str,
        _field: &str,
        _value: &str,
    ) -> Result<Vec<Document>> {
        Ok(Vec::new())
    }
}

mod common;

const COLLECTIONS: [usize; 2] = [17, 100];

fn collections(count: usize) -> Vec<CollectionVersion> {
    (0..count)
        .map(|i| {
            let fields = (0..8)
                .map(|f| {
                    FieldDescription::new(
                        format!("{f}"),
                        format!("field{f}"),
                        if f % 2 == 0 {
                            FieldKind::string()
                        } else {
                            FieldKind::int()
                        },
                    )
                })
                .collect();
            CollectionVersion::new(
                format!("C{i}"),
                format!("bafkintro{i}"),
                "introspect",
                fields,
            )
        })
        .collect()
}

fn runner(count: usize, enabled: bool) -> impl QueryExecutor {
    QueryRunner::new(NoDocs, collections(count)).with_introspection_cache(enabled)
}

const QUERY: &str = r#"{ __type(name: "C0") { name fields { name type { name } } } }"#;

fn introspection(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut group = c.benchmark_group("introspection");
    group.sample_size(20);

    for count in COLLECTIONS {
        for (name, enabled) in [("off", false), ("on", true)] {
            let runner = runner(count, enabled);
            group.bench_with_input(BenchmarkId::new(name, count), &QUERY, |b, query| {
                b.iter(|| {
                    let response =
                        rt.block_on(runner.execute(QueryRequest::new(black_box(*query))));
                    assert!(
                        response.errors.is_empty(),
                        "{name} failed: {:?}",
                        response.errors
                    );
                    black_box(response)
                })
            });
        }
    }
    group.finish();
}

criterion_group!(benches, introspection);
criterion_main!(benches);

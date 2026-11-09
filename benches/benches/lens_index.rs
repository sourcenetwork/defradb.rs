//! An indexed query on a collection that went through a lens migration.
//!
//! ```text
//! cargo bench -p benches --bench lens_index
//! ```
//!
//! A migration owes its collection a reindex; once the rebuild settles, the
//! entries describe the migrated documents and a filter on an indexed field
//! reads the index instead of scanning (#2379). The lens here passes
//! documents through unchanged, so the row measures the plan a migrated
//! collection gets, not the cost of running WASM.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use db::DB;
use document::{Document, NormalValue};
use lens::{LensConfig, LensModule, MemoryTransformStore};
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use storage::RegolithStore;

mod common;

const COLLECTION: &str = "User";
const CORPUS: [usize; 2] = [1_000, 5_000];

fn collection_version() -> CollectionVersion {
    let mut version = CollectionVersion::new(
        COLLECTION,
        "bafklensindex",
        "lensindex",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
            FieldDescription::new("3", "age", FieldKind::int(), schema::CType::LwwRegister),
        ],
    );
    version.indexes = vec![IndexDescription::new("idx_age").with_field("age", false)];
    version.is_materialized = true;
    version
}

/// `count` documents with an index on `age`, then a patch and a migration
/// onto the new version, settled before the runner is built.
async fn fixture(count: usize) -> impl QueryExecutor {
    let mut raw =
        DB::new(RegolithStore::in_memory().expect("an in-memory store")).expect("a database");
    raw.set_lens_store(Arc::new(MemoryTransformStore::new()));
    let db = Arc::new(raw);
    db.create_collection(collection_version())
        .await
        .expect("the collection to register");
    let v1 = db
        .get_collection(COLLECTION)
        .unwrap()
        .unwrap()
        .version_id()
        .to_string();

    let mutator = Arc::new(db::write::autocommit::AutoCommitMutator::new(db.clone()));
    let schema = db.get_collection(COLLECTION).unwrap().unwrap().schema_arc();
    for seq in 0..count {
        let mut doc = Document::new(schema.clone());
        doc.set("name", NormalValue::String(format!("person-{seq}")))
            .unwrap();
        doc.set("age", NormalValue::Int((seq % 80) as i64 + 18))
            .unwrap();
        mutator
            .create(COLLECTION, doc)
            .await
            .expect("the seed create to succeed");
    }

    db.patch_collection(
        &defra_core::RequestContext::default(),
        r#"[{"op":"add","path":"/User/Fields/-","value":{"Name":"level","Kind":"Int"}}]"#,
        None,
    )
    .await
    .expect("the patch to apply");
    let v2 = db
        .get_collection(COLLECTION)
        .unwrap()
        .unwrap()
        .version_id()
        .to_string();
    db.set_migration(
        LensConfig::new(
            &v1,
            &v2,
            LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
        ),
        None,
    )
    .await
    .expect("the migration to register");

    QueryRunner::with_provider(
        db::LensedAutoCommitFetcher::new(db.clone()),
        db::DbCollectionProvider::new_arc(db.clone()),
    )
    .with_mutator(mutator as Arc<dyn DocMutator>)
}

fn indexed_filter(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut group = c.benchmark_group("lens_index");
    group.sample_size(20);
    let query = "query { User(filter: {age: {_eq: 30}}) { name } }";
    for count in CORPUS {
        let runner = rt.block_on(fixture(count));
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(
            BenchmarkId::new("filter_eq_migrated", count),
            &query,
            |b, query| {
                b.iter(|| {
                    let response =
                        rt.block_on(runner.execute(QueryRequest::new(black_box(*query))));
                    assert!(response.errors.is_empty(), "{:?}", response.errors);
                    black_box(response)
                })
            },
        );
    }
    group.finish();
}

criterion_group!(benches, indexed_filter);
criterion_main!(benches);

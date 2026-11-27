//! Indexed one-to-many child pages, including parents with no children.
//!
//! `cargo bench --profile release-fast -p benches --bench child_page`
//! The old implementation returned incomplete pages. Set `CHILD_PAGE_ALLOW_INCORRECT=1`
//! only to time that baseline; the preflight still reports its incorrect output.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use db::DB;
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use storage::RegolithStore;

mod common;

const QUERY: &str = "query { Author { name published(order: {rank: ASC}, limit: 2) { rank } } }";
const EXPLAIN: &str = "query @explain(type: execute) { Author { name published(order: {rank: ASC}, limit: 2) { rank } } }";
const SDL: &str = r#"
    type Author { name: String published: [Book] }
    type Book { rank: Int @index author: Author }
"#;

async fn fixture(parents: usize, children: usize) -> impl QueryExecutor {
    let db = Arc::new(
        DB::open_from_arc(Arc::new(RegolithStore::in_memory().unwrap()))
            .await
            .unwrap(),
    );
    let versions = db
        .add_collection(&defra_core::RequestContext::ambient(), SDL)
        .await
        .unwrap();
    let version = |name: &str| versions.iter().find(|v| v.name == name).unwrap().clone();
    let (author, book) = (version("Author"), version("Book"));
    let mutator = Arc::new(db::write::autocommit::AutoCommitMutator::new(db.clone()));
    let mut ids = Vec::new();
    for i in 0..parents {
        let mut doc = Document::new(author.clone());
        doc.set("name", NormalValue::String(i.to_string())).unwrap();
        ids.push(
            mutator
                .create("Author", doc)
                .await
                .unwrap()
                .doc_id
                .to_string(),
        );
    }
    let docs = (0..children)
        .map(|i| {
            let mut doc = Document::new(book.clone());
            doc.set("rank", NormalValue::Int(i as i64)).unwrap();
            doc.set(
                "_authorID",
                NormalValue::String(ids[i % (parents / 2)].clone()),
            )
            .unwrap();
            doc
        })
        .collect();
    mutator.create_many("Book", docs).await.unwrap();
    let fetcher = db::LensedAutoCommitFetcher::new(db.clone());
    QueryRunner::new(
        fetcher,
        ["Author", "Book"]
            .iter()
            .map(|name| (*db.query_schema(name).unwrap()).clone())
            .collect(),
    )
    .with_mutator(mutator as Arc<dyn DocMutator>)
}

fn correct(data: &serde_json::Value, parents: usize, children: usize) -> bool {
    let Some(rows) = data["Author"].as_array() else {
        return false;
    };
    if rows.len() != parents {
        return false;
    }
    let mut seen = vec![false; parents];
    for row in rows {
        let Some(i) = row["name"]
            .as_str()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&i| i < parents)
        else {
            return false;
        };
        if seen[i] {
            return false;
        }
        seen[i] = true;
        let expected: Vec<_> = if i < parents / 2 {
            (i..children).step_by(parents / 2).take(2).collect()
        } else {
            Vec::new()
        };
        let Some(page) = row["published"].as_array() else {
            return false;
        };
        if page.len() != expected.len()
            || page
                .iter()
                .zip(expected)
                .any(|(doc, rank)| doc["rank"].as_u64() != Some(rank as u64))
        {
            return false;
        }
    }
    true
}

fn child_page(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let mut group = c.benchmark_group("child_page");
    group.sample_size(10);
    for (parents, children) in [(10, 100), (20, 100), (20, 400)] {
        let runner = rt.block_on(fixture(parents, children));
        let explain = rt.block_on(runner.execute(QueryRequest::new(EXPLAIN)));
        assert!(explain.errors.is_empty(), "{:?}", explain.errors);
        eprintln!(
            "child_page {parents}/{children} execution explain: {:?}",
            explain.data
        );
        let response = rt.block_on(runner.execute(QueryRequest::new(QUERY)));
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let valid = response
            .data
            .as_ref()
            .is_some_and(|data| correct(data, parents, children));
        if !valid {
            eprintln!(
                "INCORRECT child pages for {parents} parents, {children} children: {:?}",
                response.data
            );
            assert!(
                std::env::var_os("CHILD_PAGE_ALLOW_INCORRECT").is_some(),
                "refusing to benchmark incorrect results"
            );
        }
        group.bench_function(
            BenchmarkId::new(format!("parents_{parents}"), children),
            |b| {
                b.iter(|| {
                    let response = rt.block_on(runner.execute(QueryRequest::new(black_box(QUERY))));
                    assert!(response.errors.is_empty(), "{:?}", response.errors);
                    black_box(response)
                })
            },
        );
    }
    group.finish();
}
criterion_group!(benches, child_page);
criterion_main!(benches);

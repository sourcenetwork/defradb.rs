//! A relation filter on an indexed child field whose parent FK has no index,
//! over a real database (#3087).
//!
//! ```text
//! cargo bench --profile release-fast -p benches --bench join_indexed_side
//! ```
//!
//! 100,000 Books reference 2,000 Persons, 50 Books each. `Person.tier` is
//! indexed and `Book.author` is not. The filter selects one Person (50 Books),
//! 1,000 Persons (50,000 Books) or none, so the join's cost is seen for a
//! selective, a broad and an empty match set. Two reference rows give the
//! parts: an index lookup that finds no Person, and a scan of every Book
//! that keeps none.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use db::DB;
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use storage::RegolithStore;

mod common;

const PERSONS: usize = 2_000;
const BOOKS: usize = 100_000;
const BATCH: usize = 1_000;

const SDL: &str = r#"
    type Person {
        name: String
        tier: String @index
        authoredBooks: [Book]
    }
    type Book {
        title: String
        author: Person
    }
"#;

/// The tier of Person `seq`: one Person is "one", a thousand are "many".
fn tier(seq: usize) -> &'static str {
    match seq {
        0 => "one",
        1..=1_000 => "many",
        _ => "rest",
    }
}

async fn fixture() -> impl QueryExecutor {
    let store = Arc::new(RegolithStore::in_memory().expect("an in-memory store"));
    let db = Arc::new(DB::open_from_arc(store).await.expect("a database"));
    let versions = db
        .add_collection(&defra_core::RequestContext::ambient(), SDL)
        .await
        .expect("Person and Book");
    let version = |name: &str| {
        versions
            .iter()
            .find(|version| version.name == name)
            .cloned()
            .expect("the collection")
    };
    let (person, book) = (version("Person"), version("Book"));
    let mutator = Arc::new(db::write::autocommit::AutoCommitMutator::new(db.clone()));

    let mut person_ids = Vec::with_capacity(PERSONS);
    for seq in 0..PERSONS {
        let mut doc = Document::new(person.clone());
        doc.set("tier", NormalValue::String(tier(seq).to_string()))
            .unwrap();
        doc.set("name", NormalValue::String(format!("person-{seq}")))
            .unwrap();
        let created = mutator.create("Person", doc).await.expect("a Person");
        person_ids.push(created.doc_id.to_string());
    }
    for start in (0..BOOKS).step_by(BATCH) {
        let docs = (start..(start + BATCH).min(BOOKS))
            .map(|seq| {
                let mut doc = Document::new(book.clone());
                doc.set("title", NormalValue::String(format!("book-{seq}")))
                    .unwrap();
                doc.set(
                    "_authorID",
                    NormalValue::String(person_ids[seq % PERSONS].clone()),
                )
                .unwrap();
                doc
            })
            .collect();
        mutator.create_many("Book", docs).await.expect("the Books");
    }

    let fetcher = db::LensedAutoCommitFetcher::new(db.clone());
    QueryRunner::new(
        fetcher,
        ["Person", "Book"]
            .iter()
            .map(|name| (*db.query_schema(name).expect("the collection")).clone())
            .collect(),
    )
    .with_mutator(mutator as Arc<dyn DocMutator>)
}

/// Each shape's name, query, the collection it returns and its row count.
const SHAPES: [(&str, &str, &str, usize); 5] = [
    (
        "reference_person_index_miss",
        r#"query { Person(filter: {tier: {_eq: "none"}}) { name } }"#,
        "Person",
        0,
    ),
    (
        "reference_book_full_scan",
        r#"query { Book(filter: {title: {_eq: "nope"}}) { title } }"#,
        "Book",
        0,
    ),
    (
        "selective_1_author",
        r#"query { Book(filter: {author: {tier: {_eq: "one"}}}) { title } }"#,
        "Book",
        50,
    ),
    (
        "broad_1k_authors",
        r#"query { Book(filter: {author: {tier: {_eq: "many"}}}) { title } }"#,
        "Book",
        50_000,
    ),
    (
        "no_author",
        r#"query { Book(filter: {author: {tier: {_eq: "none"}}}) { title } }"#,
        "Book",
        0,
    ),
];

fn join(c: &mut Criterion) {
    let rt = common::owned_runtime();
    let runner = rt.block_on(fixture());
    let mut group = c.benchmark_group("join_indexed_side");
    group.sample_size(10);
    for (name, query, collection, rows) in SHAPES {
        group.bench_with_input(BenchmarkId::new(name, BOOKS), &query, |b, query| {
            b.iter(|| {
                let response = rt.block_on(runner.execute(QueryRequest::new(black_box(*query))));
                assert!(response.errors.is_empty(), "{name}: {:?}", response.errors);
                let found = response
                    .data
                    .as_ref()
                    .and_then(|data| data[collection].as_array())
                    .unwrap_or_else(|| panic!("{name}: no {collection} array"))
                    .len();
                assert_eq!(found, rows, "{name}");
                black_box(response)
            })
        });
    }
    group.finish();
}

criterion_group!(benches, join);
criterion_main!(benches);

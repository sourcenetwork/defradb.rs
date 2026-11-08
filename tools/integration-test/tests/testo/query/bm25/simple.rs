//! `BM25(query:, fields:)` scores documents on their `@fulltext` fields. A
//! filter on the score's alias splits the documents that match the query
//! (score above zero) from the ones that do not (score zero). BM25 is
//! defradb.rs only, so these tests do not run against Go.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

pub const RUST_CLIENTS: &[ClientType] =
    &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

const SDL: &str =
    "type Article { title: String @fulltext  body: String @fulltext  category: String }";

fn article(title: &str, body: &str) -> Actions {
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "title": title, "body": body, "category": "tech" })),
        ..Default::default()
    }
    .into()
}

/// The titles of the articles whose `score` passes `condition`.
fn titles(condition: &str, bm25: &str, expected: &[&str]) -> Actions {
    let rows: Value = expected
        .iter()
        .map(|title| m::object(json!({ "title": title })))
        .collect();
    Request {
        request: format!(
            "query {{ Article(filter: {{_alias: {{score: {condition}}}}}) {{ title score: {bm25} }} }}"
        )
        .into(),
        expected_data: json!({ "Article": rows }),
        common: Common {
            unordered_documents: vec!["/data/Article".into()],
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn case(actions: Vec<Actions>) {
    TestCase {
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(RUST_CLIENTS),
        ..Default::default()
    }
    .execute();
}

fn collection() -> Actions {
    AddCollection {
        sdl: SDL.into(),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.bm25.score")]
fn only_articles_with_the_term_score_above_zero() {
    let bm25 = r#"BM25(query: "rust", fields: ["title"])"#;
    case(vec![
        collection(),
        article(
            "Introduction to Rust",
            "Rust is a systems programming language",
        ),
        article(
            "Cooking with Python",
            "Python is a great scripting language",
        ),
        article(
            "Advanced Rust Patterns",
            "Rust ownership and borrowing patterns",
        ),
        titles(
            "{_gt: 0}",
            bm25,
            &["Introduction to Rust", "Advanced Rust Patterns"],
        ),
        titles("{_eq: 0}", bm25, &["Cooking with Python"]),
    ]);
}

#[test]
#[mantra_macros::req_test("defra.bm25.score")]
fn a_term_in_any_listed_field_scores() {
    let bm25 = r#"BM25(query: "database", fields: ["title", "body"])"#;
    case(vec![
        collection(),
        article("Hello World", "This article discusses databases"),
        article("Database Design", "Principles of good design"),
        titles("{_gt: 0}", bm25, &["Hello World", "Database Design"]),
    ]);
}

#[test]
#[mantra_macros::req_test("defra.bm25.score")]
fn an_unknown_term_scores_zero() {
    let bm25 = r#"BM25(query: "nonexistent_term_xyz", fields: ["title"])"#;
    case(vec![
        collection(),
        article("Hello World", "A simple greeting"),
        titles("{_eq: 0}", bm25, &["Hello World"]),
    ]);
}

#[test]
#[mantra_macros::req_test("defra.bm25.score")]
fn an_unaliased_score_is_named_bm25() {
    case(vec![
        collection(),
        article("Rust Programming", "Learn Rust"),
        Request {
            request: r#"query { Article { title BM25(query: "nothing", fields: ["title"]) } }"#
                .into(),
            expected_data: json!({ "Article": [{ "title": "Rust Programming", "BM25": 0.0 }] }),
            ..Default::default()
        }
        .into(),
    ]);
}

#[test]
#[mantra_macros::req_test("defra.bm25.order")]
fn ordering_by_the_score_ranks_the_best_match_first() {
    case(vec![
        collection(),
        article(
            "Auth middleware",
            "Authenticate requests through shared middleware",
        ),
        article(
            "Utility formatters",
            "Pretty print shared output for the CLI",
        ),
        Request {
            request: r#"query {
                Article(order: {_alias: {score: DESC}}) {
                    title
                    score: BM25(query: "auth middleware", fields: ["title", "body"])
                }
            }"#
            .into(),
            expected_data: json!({ "Article": [
                m::object(json!({ "title": "Auth middleware" })),
                { "title": "Utility formatters", "score": 0.0 },
            ] }),
            ..Default::default()
        }
        .into(),
    ]);
}

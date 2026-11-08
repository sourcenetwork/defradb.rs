//! `BM25(query:, fields:)` scores documents on their `@fulltext` fields: a
//! document with a query term in a listed field scores above zero, any other
//! scores zero. Scores are pinned exactly, so a zero, null or string where a
//! positive number belongs fails. BM25 is defradb.rs only, so these tests do
//! not run against Go.

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

fn request(request: &str, expected: Value) -> Actions {
    Request {
        request: request.into(),
        expected_data: expected,
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

pub fn case(id: &'static str, actions: Vec<Actions>) {
    TestCase {
        id,
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
    case(
        integration_test::scenario_id!(),
        vec![
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
            request(
                &format!("query {{ Article(order: {{title: ASC}}) {{ title score: {bm25} }} }}"),
                json!({ "Article": [
                    { "title": "Advanced Rust Patterns", "score": -1.0 },
                    { "title": "Cooking with Python", "score": 0.0 },
                    { "title": "Introduction to Rust", "score": -1.0 },
                ] }),
            ),
            titles(
                "{_gt: 0}",
                bm25,
                &["Introduction to Rust", "Advanced Rust Patterns"],
            ),
            titles("{_eq: 0}", bm25, &["Cooking with Python"]),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.bm25.score")]
fn a_term_in_any_listed_field_scores() {
    case(
        integration_test::scenario_id!(),
        vec![
            collection(),
            article("Hello World", "This article discusses databases"),
            article("Database Design", "Principles of good design"),
            request(
                r#"query {
                    Article(order: {title: ASC}) {
                        title
                        score: BM25(query: "database", fields: ["title", "body"])
                    }
                }"#,
                json!({ "Article": [
                    { "title": "Database Design", "score": -1.0 },
                    { "title": "Hello World", "score": -1.0 },
                ] }),
            ),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.bm25.score")]
fn an_unknown_term_scores_zero() {
    case(
        integration_test::scenario_id!(),
        vec![
            collection(),
            article("Hello World", "A simple greeting"),
            request(
                r#"query { Article { title score: BM25(query: "nonexistent_term_xyz", fields: ["title"]) } }"#,
                json!({ "Article": [{ "title": "Hello World", "score": 0.0 }] }),
            ),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.bm25.score")]
fn a_score_is_named_by_its_alias_or_bm25() {
    case(
        integration_test::scenario_id!(),
        vec![
            collection(),
            article("Rust Programming", "Learn Rust"),
            request(
                r#"query { Article { title relevance: BM25(query: "rust", fields: ["title"]) } }"#,
                json!({ "Article": [{ "title": "Rust Programming", "relevance": -1.0 }] }),
            ),
            request(
                r#"query { Article { title BM25(query: "rust", fields: ["title"]) } }"#,
                json!({ "Article": [{ "title": "Rust Programming", "BM25": -1.0 }] }),
            ),
        ],
    );
}

/// Ordering both ways: whatever order the documents come back in unordered,
/// it cannot match both.
#[test]
#[mantra_macros::req_test("defra.bm25.order")]
fn ordering_by_the_score_ranks_the_best_match_first() {
    let query = |direction: &str| {
        format!(
            r#"query {{
                Article(order: {{_alias: {{score: {direction}}}}}) {{
                    title
                    score: BM25(query: "auth middleware", fields: ["title", "body"])
                }}
            }}"#
        )
    };
    let auth = json!({ "title": "Auth middleware", "score": -1.0 });
    let utility = json!({ "title": "Utility formatters", "score": 0.0 });
    case(
        integration_test::scenario_id!(),
        vec![
            collection(),
            article(
                "Auth middleware",
                "Authenticate requests through shared middleware",
            ),
            article(
                "Utility formatters",
                "Pretty print shared output for the CLI",
            ),
            request(&query("DESC"), json!({ "Article": [auth, utility] })),
            request(&query("ASC"), json!({ "Article": [utility, auth] })),
        ],
    );
}

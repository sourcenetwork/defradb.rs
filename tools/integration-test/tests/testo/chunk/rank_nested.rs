//! A nested similarity on an author's articles ranks only the articles the
//! reader may read, and scores no other (`defra.chunk.rank.nested`).

use integration_test::actions::{AddCollection, AddDACPolicy, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use integration_test::USER_ACP_POLICY;
use serde_json::{json, Value};

use super::access_support::{ALICE, BOB};
use super::embed_support::{article_schema, model_embedded};
use super::rank_support::{add_article, body_about, score};

const AUTHOR: &str = "type Author { name: String articles: [Article] }";

fn author(name: &str) -> AddDoc {
    AddDoc {
        collection: "Author".into(),
        doc_map: Some(json!({ "name": name })),
        common: Common {
            capture: captures([(name, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn article_by(title: &str, topic: &str, writer: &str) -> Value {
    json!({ "title": title, "body": body_about(&[topic]), "author": m::r(writer) })
}

/// Each author with their articles scored against `vector`, as `identity`.
fn authors(vector: &str, expected: Value, identity: Option<u64>) -> Request {
    Request {
        request: format!(
            "query {{ Author(order: {{name: ASC}}) {{
                name
                articles {{ title sim: SIMILARITY(body_v: {{vector: {vector}}}) }}
            }} }}"
        )
        .into(),
        expected_data: expected,
        common: Common {
            identity_seed: identity,
            unordered: vec![
                "/data/Author/0/articles".into(),
                "/data/Author/1/articles".into(),
            ],
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.chunk.rank.nested")]
fn a_nested_ranking_scores_only_the_articles_the_reader_may_read() {
    let public = json!({ "title": "public", "sim": Value::Null });
    let alices = |title: &str| json!({ "title": title, "sim": score([1.0, 0.0, 0.0]) });
    TestCase::from_actions(vec![
        model_embedded(&[]),
        AddDACPolicy {
            policy: USER_ACP_POLICY.into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
            common: Common {
                identity_seed: Some(ALICE),
                capture: captures([("policy", "/policyID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: article_schema(Some("policy"), "dimensions: 3", "author: Author", AUTHOR),
            ..Default::default()
        }
        .into(),
        author("hidden").into(),
        author("shared").into(),
        add_article(article_by("private", "rivers", "hidden"), Some(ALICE), None),
        add_article(article_by("secret", "rivers", "shared"), Some(ALICE), None),
        // Created with no identity, so anyone may read it, and with no body, so
        // no chunk vector of a readable article can fail a wrong-length query.
        add_article(
            json!({ "title": "public", "author": m::r("shared") }),
            None,
            None,
        ),
        authors(
            "[1, 0, 0]",
            json!({ "Author": [
                { "name": "hidden", "articles": [alices("private")] },
                { "name": "shared", "articles": [alices("secret"), public.clone()] },
            ] }),
            Some(ALICE),
        )
        .into(),
        authors(
            "[1, 0, 0]",
            json!({ "Author": [
                { "name": "hidden", "articles": [] },
                { "name": "shared", "articles": [public] },
            ] }),
            Some(BOB),
        )
        .into(),
        // A wrong-length vector fails on any article with chunks it scores, so
        // this answers only because Bob's query never scores Alice's articles.
        Request {
            request: r#"query { Author(filter: {name: {_eq: "hidden"}}) {
                name
                articles { sim: SIMILARITY(body_v: {vector: [1, 0]}) }
            } }"#
                .into(),
            expected_data: json!({ "Author": [{ "name": "hidden", "articles": [] }] }),
            common: Common {
                identity_seed: Some(BOB),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

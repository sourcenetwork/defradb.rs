//! A nested similarity on an author's articles ranks only the articles the
//! reader may read, and scores no other (`defra.chunk.rank.nested`).

use integration_test::actions::{AddCollection, AddDACPolicy, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
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
        // A readable article with chunks is scored, so the same vector fails for Bob there.
        author("open").into(),
        add_article(article_by("visible", "engines", "open"), None, None),
        Request {
            request: r#"query { Author(filter: {name: {_eq: "open"}}) {
                name
                articles { sim: SIMILARITY(body_v: {vector: [1, 0]}) }
            } }"#
                .into(),
            expected_error: Some(WRONG_LENGTH.into()),
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

const WRONG_LENGTH: &str = "source and vector must be of the same length. Source: 3, Vector: 2, Field: Article__body_chunk.body_v";

/// Author `a`'s articles under `arguments`, each scored against rivers.
fn articles(arguments: Text, expected: Value) -> Request {
    Request {
        request: Text::concat([
            Text::from("query { Author { articles("),
            arguments,
            Text::from(") { title sim: SIMILARITY(body_v: {vector: [1, 0, 0]}) } } }"),
        ]),
        expected_data: json!({ "Author": [{ "articles": expected }] }),
        common: Common {
            unordered: vec!["/data/Author/0/articles".into()],
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Author `a` with articles about music (`x`), rivers, and rivers and engines (`half`).
fn three_articles() -> Vec<integration_test::actions::Actions> {
    vec![
        model_embedded(&[]),
        AddCollection {
            sdl: article_schema(None, "dimensions: 3", "author: Author", AUTHOR),
            ..Default::default()
        }
        .into(),
        author("a").into(),
        add_article(article_by("x", "music", "a"), None, Some("x")),
        add_article(article_by("rivers", "rivers", "a"), None, None),
        add_article(
            article_by("half", "rivers and engines", "a"),
            None,
            Some("half"),
        ),
    ]
}

fn x() -> Value {
    json!({ "title": "x", "sim": score([0.0, 0.0, 1.0]) })
}

fn rivers() -> Value {
    json!({ "title": "rivers", "sim": score([1.0, 0.0, 0.0]) })
}

fn half() -> Value {
    json!({ "title": "half", "sim": score([1.0, 1.0, 0.0]) })
}

#[test]
#[mantra_macros::req_test("defra.chunk.rank.nested_alias")]
fn a_nested_filter_on_scores_waits_for_the_scores() {
    let mut actions = three_articles();
    actions.extend([
        articles(
            Text::from(r#"filter: {_or: [{title: {_eq: "x"}}, {_alias: {sim: {_gt: 0.9}}}]}"#),
            json!([x(), rivers()]),
        )
        .into(),
        articles(
            Text::from("filter: {_not: {_alias: {sim: {_gt: 0.5}}}}"),
            json!([x()]),
        )
        .into(),
        articles(
            Text::concat([
                Text::from(r#"docID: [""#),
                Text::reference("half"),
                Text::from(r#""], filter: {_alias: {sim: {_gt: 0.5}}}"#),
            ]),
            json!([half()]),
        )
        .into(),
        articles(
            Text::concat([
                Text::from(r#"docID: [""#),
                Text::reference("x"),
                Text::from(r#""], filter: {_alias: {sim: {_gt: 0.5}}}"#),
            ]),
            json!([]),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.rank.nested_shape")]
fn a_nested_ranking_takes_a_filter_an_order_and_a_limit() {
    let mut actions = three_articles();
    actions.extend([
        articles(
            Text::from(
                r#"filter: {title: {_ne: "rivers"}}, order: {_alias: {sim: DESC}}, limit: 1"#,
            ),
            json!([half()]),
        )
        .into(),
        // The nested select filters, then scores, and shows the filter the query wrote.
        nested_explain(m::explain_pattern(json!({ "explain": { "operationNode": [{
            "selectTopNode": { "selectNode": { "typeIndexJoin": {
                "root": { "scanNode": {} },
                "subType": { "selectTopNode": { "selectNode": { "similarityNode": {
                    "selectNode": { "scanNode": {} },
                } } } },
            } } },
        }] } }))),
        nested_explain(m::explain_target(
            "selectNode",
            false,
            1,
            json!({ "docID": null, "filter": { "title": { "_eq": "x" } } }),
        )),
    ]);
    TestCase::from_actions(actions).execute();
}

fn nested_explain(expected: Value) -> integration_test::actions::Actions {
    Request {
        request: r#"query @explain { Author { articles(filter: {title: {_eq: "x"}}) {
            title sim: SIMILARITY(body_v: {vector: [1, 0, 0]})
        } } }"#
            .into(),
        expected_data: expected,
        ..Default::default()
    }
    .into()
}

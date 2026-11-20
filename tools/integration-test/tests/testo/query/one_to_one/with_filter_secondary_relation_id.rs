//! Generated from the conformance scenarios of tests/integration/query/one_to_one/with_filter_secondary_relation_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_eq_null_filter_on_secondary_relation_id_should_return_orphans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book With Publisher" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan Book 1" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan Book 2" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "name": "Publisher2020",
                    "_bookID": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book(filter: {_publisherID: {_eq: null}}, order: {title: ASC}) {
                            title
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        { "title": "Orphan Book 1" },
                        { "title": "Orphan Book 2" },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_neq_null_filter_on_secondary_relation_id_should_return_related() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book With Publisher" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan Book 1" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan Book 2" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "name": "Publisher2020",
                    "_bookID": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book(filter: {_publisherID: {_neq: null}}) {
                            title
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [{ "title": "Book With Publisher" }],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_eq_filter_on_secondary_relation_id_should_return_related() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book With Publisher" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan Book 1" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan Book 2" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "name": "Publisher2020",
                    "_bookID": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Book(filter: {_publisherID: {_eq: ""#),
                    Text::reference("doc1_0"),
                    Text::from(r#""}}) { title } }"#),
                ]),
                expected_data: json!({
                    "Book": [{ "title": "Book With Publisher" }],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_eq_null_filter_on_secondary_relation_id_and_other_field_should_return_matching_orphan(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book With Publisher" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan Book 1" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan Book 2" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "name": "Publisher2020",
                    "_bookID": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book(filter: {_publisherID: {_eq: null}, title: {_eq: "Orphan Book 2"}}) {
                            title
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [{ "title": "Orphan Book 2" }],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

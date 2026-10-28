//! Generated from the conformance scenarios of tests/integration/query/one_to_one/with_version_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_one.version")]
fn query_one_to_one_with_version_on_outer_after_join() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                    }

                    type Author {
                        name: String
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "فارسی دوم دبستان" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "_publishedID": m::r("doc0_0"), "name": "نمی دانم" })),
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
                        Book {
                            name
                            _version {
                                docID
                            }
                            author {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "_version": [{ "docID": m::r("doc0_0") }],
                            "author": { "name": "نمی دانم" },
                            "name": "فارسی دوم دبستان",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_one.version")]
fn query_one_to_one_with_version_on_outer_before_join() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                    }

                    type Author {
                        name: String
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "فارسی دوم دبستان" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "_publishedID": m::r("doc0_0"), "name": "نمی دانم" })),
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
                        Book {
                            name
                            author {
                                name
                            }
                            _version {
                                docID
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "_version": [{ "docID": m::r("doc0_0") }],
                            "author": { "name": "نمی دانم" },
                            "name": "فارسی دوم دبستان",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

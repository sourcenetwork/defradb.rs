//! Generated from the conformance scenarios of tests/integration/query/one_to_one_to_one/with_order_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_one_to_one.chain")]
fn query_one_to_one_to_one_with_nested_order() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Publisher {
                        name: String
                        printed: Book @primary
                    }

                    type Book {
                        name: String
                        publisher: Publisher
                        author: Author @primary
                    }

                    type Author {
                        name: String
                        published: Book
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham" })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke" })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc2_0"), "name": "Painted House" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc2_1"), "name": "Theif Lord" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "_printedID": m::r("doc1_0"), "name": "Old Publisher" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "_printedID": m::r("doc1_1"), "name": "New Publisher" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Publisher(order: {printed: {author: {name: ASC}}}) {
                                            name
                                            printed {
                                                name
                                                author {
                                                    name
                                                }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Publisher": [
                        {
                            "name": "New Publisher",
                            "printed": {
                                "author": { "name": "Cornelia Funke" },
                                "name": "Theif Lord",
                            },
                        },
                        {
                            "name": "Old Publisher",
                            "printed": {
                                "author": { "name": "John Grisham" },
                                "name": "Painted House",
                            },
                        },
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

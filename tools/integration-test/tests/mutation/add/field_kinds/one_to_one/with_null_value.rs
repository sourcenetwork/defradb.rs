//! Generated from the conformance scenarios of tests/integration/mutation/add/field_kinds/one_to_one/with_null_value_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.relation.one_to_one_link")]
fn mutation_add_one_to_one_with_explicit_null_on_primary_side() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
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
                doc_map: Some(json!({ "name": "Will Ferguson" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc1_0"), "name": "How to Be a Canadian" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Secrets at Maple Syrup Farm", "author": null })),
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
                        Book {
                            name
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
                            "author": { "name": "Will Ferguson" },
                            "name": "How to Be a Canadian",
                        },
                        { "author": null, "name": "Secrets at Maple Syrup Farm" },
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

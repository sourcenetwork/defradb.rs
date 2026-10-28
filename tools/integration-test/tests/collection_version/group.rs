//! Generated from the conformance scenarios of tests/integration/collection_version/group_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.group_order_inputs")]
fn group_by_field_for_the_many_side_in_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    {
                      __type(name: "BookField") {
                        name
                        kind
                        enumValues {
                          name
                        }
                      }
                    }
                "#
                .into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "enumValues": m::contains(json!([
                            { "name": "author" },
                            { "name": "_authorID" },
                            { "name": "_deleted" },
                            { "name": "GROUP" },
                            { "name": "_docID" },
                            { "name": "_version" },
                            { "name": "name" },
                            { "name": "rating" },
                        ])),
                        "kind": "ENUM",
                        "name": "BookField",
                    })),
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.group_order_inputs")]
fn group_by_field_for_the_single_side_in_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    {
                      __type(name: "AuthorField") {
                        name
                        kind
                        enumValues {
                          name
                        }
                      }
                    }
                "#
                .into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "enumValues": m::contains(json!([
                            { "name": "published" },
                            { "name": "_deleted" },
                            { "name": "GROUP" },
                            { "name": "_docID" },
                            { "name": "_version" },
                            { "name": "name" },
                            { "name": "age" },
                            { "name": "verified" },
                        ])),
                        "kind": "ENUM",
                        "name": "AuthorField",
                    })),
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

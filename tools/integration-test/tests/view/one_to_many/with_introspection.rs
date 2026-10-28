//! Generated from the conformance scenarios of tests/integration/view/one_to_many/with_introspection_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddView, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.introspection")]
fn view_one_to_many_gql_introspection_test() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        books: [Book]
                    }
                    type Book {
                        name: String
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    Author {
                        name
                        books {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        name: String
                        books: [BookView]
                    }
                    interface BookView {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "AuthorView") {
                            name
                            fields {
                                name
                                type {
                                    name
                                    kind
                                    ofType { name kind }
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "__type": {
                        "fields": [
                            {
                                "name": "AVG",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Float" },
                                },
                            },
                            {
                                "name": "COUNT",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Int" },
                                },
                            },
                            {
                                "name": "GROUP",
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "AuthorView" },
                                },
                            },
                            {
                                "name": "MAX",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "MIN",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "SIMILARITY",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "SUM",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Float" },
                                },
                            },
                            {
                                "name": "books",
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "BookView" },
                                },
                            },
                            {
                                "name": "name",
                                "type": { "kind": "SCALAR", "name": "String", "ofType": null },
                            },
                        ],
                        "name": "AuthorView",
                    },
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "BookView") {
                            name
                            fields {
                                name
                                type {
                                    name
                                    kind
                                    ofType { name kind }
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "__type": {
                        "fields": [
                            {
                                "name": "AVG",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Float" },
                                },
                            },
                            {
                                "name": "COUNT",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Int" },
                                },
                            },
                            {
                                "name": "GROUP",
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "BookView" },
                                },
                            },
                            {
                                "name": "MAX",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "MIN",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "SIMILARITY",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "SUM",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Float" },
                                },
                            },
                            {
                                "name": "name",
                                "type": { "kind": "SCALAR", "name": "String", "ofType": null },
                            },
                        ],
                        "name": "BookView",
                    },
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

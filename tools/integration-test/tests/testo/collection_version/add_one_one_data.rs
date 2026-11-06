//! Generated from the conformance scenarios of tests/integration/collection_version/add_one_one_data_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.mutation_inputs")]
fn add_one_to_one_input_primary_object() {
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
                        wrote: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "AuthorMutationInputArg") {
                            name
                            inputFields {
                                name
                                type {
                                    name
                                    ofType {
                                        name
                                        kind
                                    }
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "inputFields": m::contains(json!([
                            {
                                "name": "name",
                                "type": { "name": "String", "ofType": null },
                            },
                            { "name": "wrote", "type": { "name": "ID", "ofType": null } },
                            { "name": "wrote", "type": { "name": "ID", "ofType": null } },
                        ])),
                        "name": "AuthorMutationInputArg",
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
#[mantra_macros::req_test("go.collection_version.introspection.mutation_inputs")]
fn add_one_to_one_input_secondary_object() {
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
                        wrote: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "BookMutationInputArg") {
                            name
                            inputFields {
                                name
                                type {
                                    name
                                    ofType {
                                        name
                                        kind
                                    }
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "inputFields": m::contains(json!([
                            {
                                "name": "name",
                                "type": { "name": "String", "ofType": null },
                            },
                        ])),
                        "name": "BookMutationInputArg",
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

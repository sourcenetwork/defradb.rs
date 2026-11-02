//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/field/with_introspection_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.atomicity")]
fn collection_version_updates_add_field_introspection_does_not_amend_gql_types_given_bad_patch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {}
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "name", "Kind": 11} },
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 111} }
					]
				"#
                .into(),
                expected_error: Some("no type found for given name. Type: 111".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "Users") {
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
                                    "ofType": { "kind": "OBJECT", "name": "Users" },
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
                                "name": "_deleted",
                                "type": { "kind": "SCALAR", "name": "Boolean", "ofType": null },
                            },
                            {
                                "name": "_docID",
                                "type": { "kind": "SCALAR", "name": "ID", "ofType": null },
                            },
                            {
                                "name": "_version",
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "Commit" },
                                },
                            },
                        ],
                        "name": "Users",
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

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.basic")]
fn collection_version_updates_add_field_introspection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {}
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "name", "Kind": 11} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "Users") {
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
                                    "ofType": { "kind": "OBJECT", "name": "Users" },
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
                                "name": "_deleted",
                                "type": { "kind": "SCALAR", "name": "Boolean", "ofType": null },
                            },
                            {
                                "name": "_docID",
                                "type": { "kind": "SCALAR", "name": "ID", "ofType": null },
                            },
                            {
                                "name": "_version",
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "Commit" },
                                },
                            },
                            {
                                "name": "name",
                                "type": { "kind": "SCALAR", "name": "String", "ofType": null },
                            },
                        ],
                        "name": "Users",
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

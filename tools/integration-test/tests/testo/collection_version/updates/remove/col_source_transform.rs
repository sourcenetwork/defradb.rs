//! Generated from the conformance scenarios of tests/integration/collection_version/updates/remove/col_source_transform_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.patch_transform")]
fn removing_transform_preserves_old_and_new_documents() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[integration_test::ClientType::InProcess]),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String age: Int @index }".into(),
                ..Default::default()
            }.into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "old", "age": 20 })),
                ..Default::default()
            }.into(),
            PatchCollection {
                patch: r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"level","Kind":"Int"}}]"#.into(),
                lens: Some(Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "field": "age", "value": 5 }),
                        path: "bin/lenses/increment.wasm".into(),
                    }]),
                }),
                ..Default::default()
            }.into(),
            Request {
                request: "query { Users(filter: {age: {_eq: 25}}) { name age } }".into(),
                expected_data: json!({ "Users": [{ "name": "old", "age": 25 }] }),
                ..Default::default()
            }.into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "new", "age": 40 })),
                ..Default::default()
            }.into(),
            PatchCollection {
                patch: r#"[{"op":"remove","path":"/Users/PreviousVersion/Transform"}]"#.into(),
                ..Default::default()
            }.into(),
            Request {
                request: "query { Users(filter: {age: {_eq: 20}}) { name age } }".into(),
                expected_data: json!({ "Users": [{ "name": "old", "age": 20 }] }),
                ..Default::default()
            }.into(),
            Request {
                request: "query { Users(filter: {age: {_eq: 40}}) { name age } }".into(),
                expected_data: json!({ "Users": [{ "name": "new", "age": 40 }] }),
                ..Default::default()
            }.into(),
        ],
        ..Default::default()
    }.execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.patch_transform")]
fn col_version_update_remove_collection_source_transform() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} }
					]
				"#
                .into(),
                lens: Some(Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "name", "value": "Fred" }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                }),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "/Users/PreviousVersion/Transform"
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

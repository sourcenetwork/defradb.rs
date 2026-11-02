//! Generated from the conformance scenarios of tests/integration/collection_version/updates/replace/query_source_query_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, AddView, PatchCollection, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.view_sources")]
fn col_version_update_replace_query_source_query_name() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Books {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    Books {
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type View @materialized(if: false) {
                        name: String
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
						{
							"op": "replace",
							"path": "/View/Query/Query/Name",
							"value": "Users"
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
                                        View {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "View": [{ "name": "Shahzad" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.view_sources")]
fn col_version_update_replace_query_source_query() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Books {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    Books {
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type View @materialized(if: false) {
                        name: String
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
						{
							"op": "replace",
							"path": "/View/Query/Query",
							"value": {"Name": "Users", "Fields":[{"Name":"name"}]}
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
                                        View {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "View": [{ "name": "Shahzad" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

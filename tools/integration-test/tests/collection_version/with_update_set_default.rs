//! Generated from the conformance scenarios of tests/integration/collection_version/with_update_set_default_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, PatchCollection, Request, SetActiveCollectionVersion,
};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.active_version")]
fn collection_version_with_update_and_set_default_version_to_empty_string_errors() {
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
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "".into(),
                expected_error: Some("collection version ID can't be empty".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.active_version")]
fn collection_version_with_update_and_set_default_version_to_new_allows_querying_of_new_field() {
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
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} },
						{ "op": "replace", "path": "/Users/IsActive", "value": false }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreigvzkfdc4y2ppvvpmmdw3t7kv4nd5dgfh5jfytef3kbzem6po55zu".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            email
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.active_version")]
fn collection_version_with_update_and_set_default_version_to_original_new_field_is_not_queriable() {
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
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} },
						{ "op": "replace", "path": "/Users/IsActive", "value": false }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            email
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(r#"Cannot query field "email" on type "Users"."#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.active_version")]
fn collection_version_with_update_and_set_default_version_to_unknown_version_errors() {
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
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "does not exist".into(),
                expected_error: Some("collection not found".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

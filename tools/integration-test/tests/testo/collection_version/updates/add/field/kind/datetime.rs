//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/field/kind/datetime_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.other_scalars")]
fn collection_version_updates_add_field_kind_date_time_substitution_with_add() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "foo", "Kind": "DateTime"} }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "foo": "2017-07-23T03:46:56-05:00" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            foo
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "foo": "2017-07-23T03:46:56-05:00", "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.other_scalars")]
fn collection_version_updates_add_field_kind_date_time_with_add() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "foo", "Kind": 10} }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "foo": "2017-07-23T03:46:56-05:00" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            foo
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "foo": "2017-07-23T03:46:56-05:00", "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.other_scalars")]
fn collection_version_updates_add_field_kind_date_time() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "foo", "Kind": 10} }
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
                                            foo
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

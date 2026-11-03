//! Generated from the conformance scenarios of tests/integration/collection_version/updates/replace/materialized_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, AddView, GetCollections, PatchCollection, Request, Truncate,
};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.immutable_settings")]
fn col_version_update_replace_is_materialized_given_false_and_collection_errors() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
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
						{
							"op": "replace",
							"path": "/bafyreibhpgygzsmki22sql5ejzcojrrxbc5iuhpydhdzxul5w2znc7zrgu/IsMaterialized",
							"value": false
						}
					]
				"#
                .into(),
                expected_error: Some(
                    "non-materialized collections are not supported. Collection: User".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.dematerialize")]
fn col_version_update_replace_is_materialized_given_false_and_view_errors_if_not_truncated() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        name
                    }
                "#.into(),
                sdl: r#"
                    type UserView @materialized(if: true) {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "/UserView/IsMaterialized",
							"value": false
						}
					]
				"#.into(),
                expected_error: Some("cannot dematerialize a materialized view that has data, first truncate it and then try again.".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.dematerialize")]
fn col_version_update_replace_is_materialized_given_false_and_view() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "UserView".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "/UserView/IsMaterialized",
							"value": false
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("UserView".into()),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "UserView",
                            "IsActive": true,
                            "IsMaterialized": false,
                            "IsBranchable": false,
                            "PreviousVersion": null,
                        })),
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }, { "name": "Fred" }] }),
                common: Common {
                    unordered_documents: vec!["/data/User".into()],
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

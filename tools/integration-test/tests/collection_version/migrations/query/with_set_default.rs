//! Generated from the conformance scenarios of tests/integration/collection_version/migrations/query/with_set_default_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, ConfigureMigration, GetCollections, PatchCollection, Request,
    SetActiveCollectionVersion,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.active_version")]
fn collection_migration_query_with_set_default_to_latest_applies_forward_migration() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} },
						{ "op": "replace", "path": "/Users/IsActive", "value": false }
					]
				"#
                .into(),
                lens: Some(Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "verified", "value": true }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                }),
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreidwvvr7kp5rqt7dbgzw55vuueovkjz6b2mlvz3rq2pxf22fqenzdm".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            verified
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John", "verified": true }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.active_version")]
fn collection_migration_query_with_set_default_to_original_applies_inverse_migration() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} },
						{ "op": "replace", "path": "/Users/IsActive", "value": false }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreidwvvr7kp5rqt7dbgzw55vuueovkjz6b2mlvz3rq2pxf22fqenzdm".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "verified": true })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "bafyreiabmrtgxy5dgotuc53gfaamuqhlzugyeetzbuv7s3x6ufmlr5ylga"
                    .into(),
                destination_version_id:
                    "bafyreidwvvr7kp5rqt7dbgzw55vuueovkjz6b2mlvz3rq2pxf22fqenzdm".into(),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "verified", "value": true }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreiabmrtgxy5dgotuc53gfaamuqhlzugyeetzbuv7s3x6ufmlr5ylga".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            verified
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John", "verified": null }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.incremental")]
fn collection_migration_query_with_set_default_to_original_version_that_doc_was_added_at_clears_migrations(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    capture: captures([("version0", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "verified": false })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} },
						{ "op": "replace", "path": "/Users/IsActive", "value": true }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    capture: captures([("version1", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: Text::reference("version0"),
                destination_version_id: Text::reference("version1"),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "verified", "value": true }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: Text::reference("version0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            verified
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John", "verified": false }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

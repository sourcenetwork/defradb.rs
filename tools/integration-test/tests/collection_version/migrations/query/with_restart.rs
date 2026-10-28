//! Generated from the conformance scenarios of tests/integration/collection_version/migrations/query/with_restart_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, ConfigureMigration, NodeOpenPersistent, PatchCollection, Request,
    Restart,
};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.restart")]
fn collection_migration_query_with_restart_and_migration_before_patch_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenPersistent {
                ..Default::default()
            }
            .into(),
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
            ConfigureMigration {
                source_version_id: "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu"
                    .into(),
                destination_version_id:
                    "bafyreigqfjat435ghyt66tdaucp7oi2mke5jafx3jw3rozanopihr2vf44".into(),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "verified", "value": true }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            Restart {
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "verified", "Kind": "Boolean"} }
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
#[mantra_macros::req_test("go.collection_version.migrations.query.restart")]
fn collection_migration_query_with_restart() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenPersistent {
                ..Default::default()
            }
            .into(),
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "verified", "Kind": "Boolean"} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu"
                    .into(),
                destination_version_id:
                    "bafyreigqfjat435ghyt66tdaucp7oi2mke5jafx3jw3rozanopihr2vf44".into(),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "verified", "value": true }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            Restart {
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

//! Generated from the conformance scenarios of tests/integration/collection_version/migrations/query/with_inverse_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, ConfigureMigration, PatchCollection, Request, SetActiveCollectionVersion,
};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.active_version")]
fn collection_migration_query_inverses_across_multiple_versions() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        height: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "verified", "Kind": "Boolean"} }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "bafyreigbatez5rnojqa4ccfqsbguh4ckquxr76elgqij7ckftbxpwqniv4".into(),
                destination_version_id: "bafyreigl262d3mo7rswhwwpymsioyr6d6stzxqqaf5vpkq4ahxrl6owrmu".into(),
                lens: Lens {
                    lenses: Some(vec![
                        LensModule {
                            arguments: json!({ "dst": "age", "value": 30 }),
                            path: "bin/lenses/set-default.wasm".into(),
                        },
                    ]),
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "bafyreigl262d3mo7rswhwwpymsioyr6d6stzxqqaf5vpkq4ahxrl6owrmu".into(),
                destination_version_id: "bafyreihiiez4vcgh4rys2zfs74macgwyybchutjslyw2oin747enuywn54".into(),
                lens: Lens {
                    lenses: Some(vec![
                        LensModule {
                            arguments: json!({ "dst": "height", "value": 190 }),
                            path: "bin/lenses/set-default.wasm".into(),
                        },
                    ]),
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 33, "height": 185 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreigbatez5rnojqa4ccfqsbguh4ckquxr76elgqij7ckftbxpwqniv4".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            age
                                            height
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "age": null, "height": null, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

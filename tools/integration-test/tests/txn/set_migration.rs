//! Generated from the conformance scenarios of tests/integration/txn/set_migration_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, ConfigureMigration, PatchCollection, Request, TxnBegin, TxnDiscard,
};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.txn.definitions.uncommitted-invisible")]
fn txn_set_migration_without_commit_succeeds() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
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
            TxnBegin {
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                txn: Some(0),
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
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

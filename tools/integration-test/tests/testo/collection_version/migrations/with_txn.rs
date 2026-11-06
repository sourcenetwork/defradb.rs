//! Generated from the conformance scenarios of tests/integration/collection_version/migrations/with_txn_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{ConfigureMigration, GetCollections, TxnBegin, TxnDiscard};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.configure")]
fn collection_migration_get_migrations_with_txn() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            TxnBegin {
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "does not exist".into(),
                destination_version_id: "also does not exist".into(),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "verified", "value": false }),
                    }]),
                },
                txn: Some(0),
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                get_inactive: Some(true),
                txn: Some(0),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "",
                            "IsMaterialized": true,
                            "IsActive": false,
                            "IsBranchable": false,
                            "VersionID": "also does not exist",
                            "PreviousVersion": m::object(json!({
                                "SourceCollectionID": "does not exist",
                                "Transform": m::r("lens0"),
                            })),
                        })),
                        m::object(json!({
                            "Name": "",
                            "IsMaterialized": true,
                            "IsActive": false,
                            "IsBranchable": false,
                            "VersionID": "does not exist",
                            "PreviousVersion": null,
                        })),
                    ],
                }),
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

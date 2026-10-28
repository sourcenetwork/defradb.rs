//! Generated from the conformance scenarios of tests/integration/collection_version/migrations/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, ConfigureMigration, GetCollectionsFiltered, PatchCollection,
};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.configure")]
fn collection_migration_configure_migration_skipping_version_errors() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users { }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "name", "Kind": "Boolean"} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "verified", "Kind": "String"} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna"
                    .into(),
                destination_version_id:
                    "bafyreih3uwvq6u5yqt65os3u5jdrrmy6gfi7wjq3vwvnm45jhjodbablhe".into(),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "verified", "value": false }),
                    }]),
                },
                expected_error: Some(
                    "cannot migrate between non-adjacent collection versions".into(),
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
#[mantra_macros::req_test("go.collection_version.migrations.configure")]
fn collection_migration_does_not_error_given_unknown_collection_roots() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            ConfigureMigration {
                source_version_id: "does not exist".into(),
                destination_version_id: "also does not exist".into(),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        path: "bin/lenses/set-default.wasm".into(),
                        arguments: json!({ "dst": "verified", "value": false }),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                get_inactive: Some(true),
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.configure")]
fn collection_migration_get_migrations_returns_multiple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            ConfigureMigration {
                source_version_id: "does not exist".into(),
                destination_version_id: "also does not exist".into(),
                lens: Lens {
                    lenses: Some(vec![
                        LensModule {
                            path: "bin/lenses/set-default.wasm".into(),
                            arguments: json!({ "dst": "verified", "value": false }),
                        },
                    ]),
                },
                common: Common { capture: captures([("lens0", "/lensID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "bafyreigsld6ten2pppcu2tgkbexqwdndckp6zt2vfjhuuheykqkgpmwk7i".into(),
                destination_version_id: "bafyreigqfjat435ghyt66tdaucp7oi2mke5jafx3jw3rozanopihr2vf44".into(),
                lens: Lens {
                    lenses: Some(vec![
                        LensModule {
                            path: "bin/lenses/set-default.wasm".into(),
                            arguments: json!({ "dst": "verified", "value": true }),
                        },
                    ]),
                },
                common: Common { capture: captures([("lens1", "/lensID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                get_inactive: Some(true),
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
                            "VersionID": "bafyreigqfjat435ghyt66tdaucp7oi2mke5jafx3jw3rozanopihr2vf44",
                            "PreviousVersion": m::object(json!({
                                "SourceCollectionID": "bafyreigsld6ten2pppcu2tgkbexqwdndckp6zt2vfjhuuheykqkgpmwk7i",
                                "Transform": m::r("lens1"),
                            })),
                        })),
                        m::object(json!({
                            "Name": "",
                            "IsMaterialized": true,
                            "IsActive": false,
                            "IsBranchable": false,
                            "VersionID": "bafyreigsld6ten2pppcu2tgkbexqwdndckp6zt2vfjhuuheykqkgpmwk7i",
                            "PreviousVersion": null,
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.configure")]
fn collection_migration_replaces_existing_migation_based_on_source_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            ConfigureMigration {
                source_version_id: "a".into(),
                destination_version_id: "b".into(),
                lens: Lens {
                    lenses: Some(vec![
                        LensModule {
                            path: "bin/lenses/set-default.wasm".into(),
                            arguments: json!({ "dst": "verified", "value": false }),
                        },
                    ]),
                },
                common: Common { capture: captures([("lens0", "/lensID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "a".into(),
                destination_version_id: "c".into(),
                lens: Lens {
                    lenses: Some(vec![
                        LensModule {
                            path: "bin/lenses/set-default.wasm".into(),
                            arguments: json!({ "dst": "age", "value": 123 }),
                        },
                    ]),
                },
                common: Common { capture: captures([("lens1", "/lensID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                get_inactive: Some(true),
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "",
                            "IsMaterialized": true,
                            "IsActive": false,
                            "IsBranchable": false,
                            "VersionID": "a",
                            "PreviousVersion": null,
                        })),
                        m::object(json!({
                            "Name": "",
                            "IsMaterialized": true,
                            "IsActive": false,
                            "IsBranchable": false,
                            "VersionID": "b",
                            "PreviousVersion": m::object(json!({ "SourceCollectionID": "a", "Transform": m::r("lens0") })),
                        })),
                        m::object(json!({
                            "Name": "",
                            "IsMaterialized": true,
                            "IsActive": false,
                            "IsBranchable": false,
                            "VersionID": "c",
                            "PreviousVersion": m::object(json!({ "SourceCollectionID": "a", "Transform": m::r("lens1") })),
                        })),
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

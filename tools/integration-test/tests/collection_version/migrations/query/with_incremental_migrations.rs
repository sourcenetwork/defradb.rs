//! Generated from the conformance scenarios of tests/integration/collection_version/migrations/query/with_incremental_migrations_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, ConfigureMigration, GetCollections, NodeOpenPersistent, PatchCollection,
    Request, Restart,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.incremental")]
fn collection_migration_query_intermediate_migration_registered_last_should_produce_fully_migrated_results(
) {
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
            GetCollections {
                common: Common {
                    capture: captures([("version1", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    capture: captures([("version2", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "level", "Kind": "Int"} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    capture: captures([("version3", "/collections/0/VersionID")]),
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
            ConfigureMigration {
                source_version_id: Text::reference("version2"),
                destination_version_id: Text::reference("version3"),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "level", "value": 42 }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: Text::reference("version1"),
                destination_version_id: Text::reference("version2"),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "email", "value": "ilovewasm@source.com" }),
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
                                            email
                                            level
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "email": "ilovewasm@source.com",
                            "level": 42,
                            "name": "John",
                            "verified": true,
                        },
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
#[mantra_macros::req_test("go.collection_version.migrations.query.incremental")]
fn collection_migration_query_intermediate_stamp_survives_restart_should_remigrate_after_restart() {
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
            GetCollections {
                common: Common {
                    capture: captures([("version1", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    capture: captures([("version2", "/collections/0/VersionID")]),
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
            Restart {
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: Text::reference("version1"),
                destination_version_id: Text::reference("version2"),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "email", "value": "ilovewasm@source.com" }),
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
                                            email
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "email": "ilovewasm@source.com",
                            "name": "John",
                            "verified": true,
                        },
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
#[mantra_macros::req_test("go.collection_version.migrations.query.incremental")]
fn collection_migration_query_sparse_chain_with_index_and_forward_migration_should_migrate_and_reindex_correctly(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 20, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 30, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 25, "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 32, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
				[
					{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "level", "Kind": "Int"} }
				]
			"#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
				[
					{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "points", "Kind": "Int"} }
				]
			"#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
				[
					{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "rank", "Kind": "Int"} }
				]
			"#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
				[
					{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "score", "Kind": "Int"} }
				]
			"#
                .into(),
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "bafyreidmsarf4ac4eihxk3ocqfort3e3pxhb7eumatvkanjsxxkjrn3h6a"
                    .into(),
                destination_version_id:
                    "bafyreidptieeo3tckkyi6jnomavo3noy2mxuv7dfuc76pf2vgxm6ilfazq".into(),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "field": "age", "value": 5 }),
                        path: "bin/lenses/increment.wasm".into(),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {age: {_eq: 30}}) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "age": 30, "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        Users(filter: {age: {_eq: 30}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
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
fn collection_migration_query_three_incremental_migrations_should_migrate_across_all_three() {
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
            GetCollections {
                common: Common {
                    capture: captures([("version1", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    capture: captures([("version2", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "level", "Kind": "Int"} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    capture: captures([("version3", "/collections/0/VersionID")]),
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
            ConfigureMigration {
                source_version_id: Text::reference("version1"),
                destination_version_id: Text::reference("version2"),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "email", "value": "ilovewasm@source.com" }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: Text::reference("version2"),
                destination_version_id: Text::reference("version3"),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "level", "value": 42 }),
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
                                            email
                                            level
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "email": "ilovewasm@source.com",
                            "level": 42,
                            "name": "John",
                            "verified": true,
                        },
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

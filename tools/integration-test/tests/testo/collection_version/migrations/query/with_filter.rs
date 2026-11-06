//! Generated from the conformance scenarios of tests/integration/collection_version/migrations/query/with_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, ConfigureMigration, PatchCollection, Request,
};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.filters")]
fn collection_migration_query_with_filter_and_migration_between_old_versions_should_apply_migration(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 20, "name": "Andy" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 30, "name": "John" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 25, "name": "Fred" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 32, "name": "Islam" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "level", "Kind": "Int"} }]"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "points", "Kind": "Int"} }]"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "rank", "Kind": "Int"} }]"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "score", "Kind": "Int"} }]"#.into(),
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "bafyreidmsarf4ac4eihxk3ocqfort3e3pxhb7eumatvkanjsxxkjrn3h6a".into(),
                destination_version_id: "bafyreidptieeo3tckkyi6jnomavo3noy2mxuv7dfuc76pf2vgxm6ilfazq".into(),
                lens: Lens {
                    lenses: Some(vec![
                        LensModule {
                            arguments: json!({ "field": "age", "value": 5 }),
                            path: "bin/lenses/increment.wasm".into(),
                        },
                    ]),
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {age: {_eq: 30}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {age: {_eq: 35}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.filters")]
fn collection_migration_query_with_filter_and_migration_in_old_patch_should_apply_migration2() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 20, "name": "Andy" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 30, "name": "John" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 25, "name": "Fred" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 32, "name": "Islam" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "level", "Kind": "Int"} }]"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "points", "Kind": "Int"} }]"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "rank", "Kind": "Int"} }]"#.into(),
                lens: Some(Lens {
                    lenses: Some(vec![
                        LensModule {
                            arguments: json!({ "field": "age", "value": 5 }),
                            path: "bin/lenses/increment.wasm".into(),
                        },
                    ]),
                }),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "score", "Kind": "Int"} }]"#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {age: {_eq: 30}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {age: {_eq: 35}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.filters")]
fn collection_migration_query_with_filter_should_filter_f_migration() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 20, "name": "Andy" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 30, "name": "John" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 25, "name": "Fred" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 32, "name": "Islam" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "level", "Kind": "Int"} }]"#.into(),
                lens: Some(Lens {
                    lenses: Some(vec![
                        LensModule {
                            arguments: json!({ "field": "age", "value": 5 }),
                            path: "bin/lenses/increment.wasm".into(),
                        },
                    ]),
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {age: {_eq: 30}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {age: {_eq: 35}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

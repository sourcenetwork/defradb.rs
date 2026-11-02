//! Generated from the conformance scenarios of tests/integration/collection_version/migrations/query/with_doc_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, ConfigureMigration, PatchCollection, Request,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.filters")]
fn collection_migration_query_by_doc_id() {
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
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
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
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#"") {
						name
						verified
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "Shahzad", "verified": true }] }),
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
fn collection_migration_query_multiple_queries_by_doc_id() {
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
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Dave" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
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
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#"") {
						name
						verified
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "Shahzad", "verified": true }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (docID: ""#,
                    ),
                    Text::reference("doc0_1"),
                    Text::from(
                        r#"") {
						name
						verified
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "Fred", "verified": true }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (docID: ""#,
                    ),
                    Text::reference("doc0_2"),
                    Text::from(
                        r#"") {
						name
						verified
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "Chris", "verified": true }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (docID: ""#,
                    ),
                    Text::reference("doc0_3"),
                    Text::from(
                        r#"") {
						name
						verified
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John", "verified": true }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (docID: ""#,
                    ),
                    Text::reference("doc0_4"),
                    Text::from(
                        r#"") {
						name
						verified
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "Islam", "verified": true }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (docID: ""#,
                    ),
                    Text::reference("doc0_5"),
                    Text::from(
                        r#"") {
						name
						verified
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "Dave", "verified": true }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

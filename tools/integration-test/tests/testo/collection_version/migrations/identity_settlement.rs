//! Identity-path settlement must not erase a future migration's work.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, ConfigureMigration, GetCollections, PatchCollection, Request,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.index.reindex")]
fn identity_settlement_before_transform_should_apply_later_migration() {
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
            GetCollections {
                filtered: true,
                common: Common {
                    capture: captures([("version1", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 20, "name": "Andy" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 25, "name": "Fred" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 30, "name": "John" })),
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
                filtered: true,
                common: Common {
                    capture: captures([("version2", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: Text::reference("version1"),
                destination_version_id: Text::reference("version2"),
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
                        }
                    }
                "#
                .into(),
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
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

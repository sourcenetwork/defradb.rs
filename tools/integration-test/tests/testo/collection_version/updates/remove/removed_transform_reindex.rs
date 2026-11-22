//! Removing a source transform reindexes the values an index already holds.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.collection_version.removed_transform_reindex")]
fn remove_source_transform_reindexes_an_indexed_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} }
                    ]
                "#
                .into(),
                lens: Some(Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "name", "value": "Fred" }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                }),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "remove", "path": "/Users/PreviousVersion/Transform" }
                    ]
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users(filter: {name: {_eq: "Shahzad"}}) {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

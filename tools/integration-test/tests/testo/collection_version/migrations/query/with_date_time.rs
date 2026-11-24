//! A migrated DateTime renders as Go's `time.RFC3339Nano` does.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.collection_version.migrations.query.date_time_format")]
fn collection_migration_query_renders_migrated_date_times_without_trailing_zeros() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        required: DateTime! @index
                        optional: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({
                    "name": "whole",
                    "required": "2026-08-01T12:34:56Z",
                    "optional": "1999-12-31T23:59:59Z"
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({
                    "name": "fraction",
                    "required": "2026-08-02T12:34:56.120Z",
                    "optional": "1999-12-31T23:59:59.000000001Z"
                })),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "migrated", "Kind": "Boolean"} }
                    ]
                "#
                .into(),
                lens: Some(Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "migrated", "value": true }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users(order: {name: ASC}) {
                            name
                            required
                            optional
                            migrated
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "name": "fraction",
                            "required": "2026-08-02T12:34:56.12Z",
                            "optional": "1999-12-31T23:59:59.000000001Z",
                            "migrated": true
                        },
                        {
                            "name": "whole",
                            "required": "2026-08-01T12:34:56Z",
                            "optional": "1999-12-31T23:59:59Z",
                            "migrated": true
                        }
                    ]
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

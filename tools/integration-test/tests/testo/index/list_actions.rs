//! Generated from the conformance scenarios of tests/integration/index/list_actions_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{AddCollection, AddDoc, ListActions, NewIndex};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.definition.status")]
fn index_backfill_after_successful_build_lists_no_action() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: false,
                    fields: vec![IndexField {
                        name: "name".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListActions {
                expected_data: json!({ "actions": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

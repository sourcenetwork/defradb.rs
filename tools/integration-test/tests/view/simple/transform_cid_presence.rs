//! Generated from the conformance scenarios of tests/integration/view/simple/transform_cid_presence_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddView, GetCollectionsFiltered};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.transform_cid")]
fn add_view_blank_transform_cid_fails_and_creates_no_view() {
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
            AddView {
                query: r#"
                    User {
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        name: String
                    }
                "#
                .into(),
                transform_cid: Some("".into()),
                expected_error: Some("cid too short".into()),
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                collection_name: Some("UserView".into()),
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

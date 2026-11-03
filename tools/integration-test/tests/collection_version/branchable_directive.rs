//! Generated from the conformance scenarios of tests/integration/collection_version/branchable_directive_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollections};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.define.branchable")]
fn col_version_branchable_argument_if_false_should_be_false() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable(if: false) {}
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "Users",
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "PreviousVersion": m::optional(json!(null)),
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
#[mantra_macros::req_test("go.collection_version.define.branchable")]
fn col_version_branchable_argument_if_true_should_be_true() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable(if: true) {}
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "Users",
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": true,
                            "PreviousVersion": m::optional(json!(null)),
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
#[mantra_macros::req_test("go.collection_version.define.branchable")]
fn col_version_branchable_no_arguments_default_true() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable {}
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "Users",
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": true,
                            "PreviousVersion": m::optional(json!(null)),
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

//! Generated from the conformance scenarios of tests/integration/acp/nac/delete_index_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, DeleteIndex, NodeStopForRestart, Start};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.indexes")]
fn nac_gates_delete_index_authorized_identity_allow_access() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                    }
                "#
                .into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                collection: "User".into(),
                index_name: "User_name_ASC".into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.indexes")]
fn nac_gates_delete_index_no_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                    }
                "#
                .into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                collection: "User".into(),
                index_name: "User_name_ASC".into(),
                expected_error: Some(
                    "not authorized to perform operation. Permission: get-collection".into(),
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
#[mantra_macros::req_test("go.acp.nac.gates.indexes")]
fn nac_gates_delete_index_wrong_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                    }
                "#
                .into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                collection: "User".into(),
                index_name: "User_name_ASC".into(),
                expected_error: Some(
                    "not authorized to perform operation. Permission: get-collection".into(),
                ),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

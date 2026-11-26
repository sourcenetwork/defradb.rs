//! Ported from the conformance scenarios of tests/integration/acp/nac/set_active_collection_version_test.go at defradb e152b5cab.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, Close, PatchCollection, SetActiveCollectionVersion, Start,
};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.collection-schema")]
fn nac_gates_set_active_collection_version_authorized_identity_allow_access() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
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
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} },
                        { "op": "replace", "path": "/Users/IsActive", "value": false }
                    ]
                "#
                .into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreigvzkfdc4y2ppvvpmmdw3t7kv4nd5dgfh5jfytef3kbzem6po55zu".into(),
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
#[mantra_macros::req_test("go.acp.nac.gates.collection-schema")]
fn nac_gates_set_active_collection_version_no_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
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
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} },
                        { "op": "replace", "path": "/Users/IsActive", "value": false }
                    ]
                "#
                .into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreigvzkfdc4y2ppvvpmmdw3t7kv4nd5dgfh5jfytef3kbzem6po55zu".into(),
                expected_error: Some(
                    "not authorized to perform operation. Permission: patch-collection".into(),
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
#[mantra_macros::req_test("go.acp.nac.gates.collection-schema")]
fn nac_gates_set_active_collection_version_wrong_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
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
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} },
                        { "op": "replace", "path": "/Users/IsActive", "value": false }
                    ]
                "#
                .into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreigvzkfdc4y2ppvvpmmdw3t7kv4nd5dgfh5jfytef3kbzem6po55zu".into(),
                expected_error: Some(
                    "not authorized to perform operation. Permission: patch-collection".into(),
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

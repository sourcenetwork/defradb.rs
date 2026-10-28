//! Generated from the conformance scenarios of tests/integration/acp/nac/delete_document_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, DeleteDoc, NodeStopForRestart, PeerEventsStart, Request, Start,
    WaitForUpdate,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.documents")]
fn nac_gates_delete_document_authorized_identity_allow_access() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
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
                    type User {
                        name: String
                        age: Int
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
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 48 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query{ User { name } }".into(),
                expected_data: json!({ "User": [] }),
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
#[mantra_macros::req_test("go.acp.nac.gates.documents")]
fn nac_gates_delete_document_no_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
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
                    type User {
                        name: String
                        age: Int
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
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 48 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                expected_error: Some(
                    "not authorized to perform operation. Permission: get-collection".into(),
                ),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query{ User { name } }".into(),
                expected_data: json!({ "User": [{ "name": "Shahzad" }] }),
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
#[mantra_macros::req_test("go.acp.nac.gates.documents")]
fn nac_gates_delete_document_wrong_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
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
                    type User {
                        name: String
                        age: Int
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
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 48 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                expected_error: Some(
                    "not authorized to perform operation. Permission: get-collection".into(),
                ),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query{ User { name } }".into(),
                expected_data: json!({ "User": [{ "name": "Shahzad" }] }),
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

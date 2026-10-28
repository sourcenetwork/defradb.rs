//! Generated from the conformance scenarios of tests/integration/acp/nac/verify_signature_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, NodeOpenHTTPSigning, NodeStopForRestart, PeerEventsStart,
    SignatureVerifySeed, Start, WaitForUpdate,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.signature")]
fn nac_gates_verify_signature_authorized_identity_allow_access() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenHTTPSigning {
                ..Default::default()
            }
            .into(),
            NodeStopForRestart {
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                enable_signing: Some(true),
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
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                common: Common {
                    capture: captures([("head", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SignatureVerifySeed {
                signer_seed: 0,
                cid: Text::reference("head"),
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
#[mantra_macros::req_test("go.acp.nac.gates.signature")]
fn nac_gates_verify_signature_go_client_authorized_identity_allow_access() {
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
                enable_signing: Some(true),
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
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                common: Common {
                    capture: captures([("head", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SignatureVerifySeed {
                signer_seed: 1,
                cid: Text::reference("head"),
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
#[mantra_macros::req_test("go.acp.nac.gates.signature")]
fn nac_gates_verify_signature_no_identity_not_authorized_error() {
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
                enable_signing: Some(true),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SignatureVerifySeed {
                signer_seed: 0,
                cid: "bafyreihymej6gbxq7qauy4tgt37di25uap2ahzq7z5d3ln3og5syo7rwmi".into(),
                expected_error: Some(
                    "not authorized to perform operation. Permission: verify-signature".into(),
                ),
                ..Default::default()
            }
            .into(),
            SignatureVerifySeed {
                signer_seed: 1,
                cid: "bafyreihymej6gbxq7qauy4tgt37di25uap2ahzq7z5d3ln3og5syo7rwmi".into(),
                expected_error: Some(
                    "not authorized to perform operation. Permission: verify-signature".into(),
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
#[mantra_macros::req_test("go.acp.nac.gates.signature")]
fn nac_gates_verify_signature_wrong_identity_not_authorized_error() {
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
                enable_signing: Some(true),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SignatureVerifySeed {
                signer_seed: 0,
                cid: "bafyreihymej6gbxq7qauy4tgt37di25uap2ahzq7z5d3ln3og5syo7rwmi".into(),
                expected_error: Some(
                    "not authorized to perform operation. Permission: verify-signature".into(),
                ),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SignatureVerifySeed {
                signer_seed: 1,
                cid: "bafyreihymej6gbxq7qauy4tgt37di25uap2ahzq7z5d3ln3og5syo7rwmi".into(),
                expected_error: Some(
                    "not authorized to perform operation. Permission: verify-signature".into(),
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

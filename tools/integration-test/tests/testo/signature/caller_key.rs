//! An in-process write signs with the acting client identity's own key, and
//! with the node's key when the request carries no identity, as Go's
//! `contextForSigning` does.

use integration_test::actions::shared::KeyType;
use integration_test::actions::{
    AddCollection, AddDoc, NewNode, UpdateDoc, VerifyBlockSignature, WaitForPeersEvents,
    WaitForSync,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const OTHER_KEY: &str = "signature was created by a different key";

fn verify(cid: &str, signer_seed: Option<u64>, error: Option<&str>) -> VerifyBlockSignature {
    VerifyBlockSignature {
        signer_seed,
        signer_node: signer_seed.is_none().then(|| "db".to_owned()),
        cid: Text::reference(cid),
        expected_error: error.map(str::to_owned),
        ..Default::default()
    }
}

fn head(capture: &'static str) -> WaitForSync {
    WaitForSync {
        doc_id: Some(Text::reference("doc")),
        common: Common {
            capture: captures([(capture, "/cid")]),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.signature.embedded_caller_key")]
fn signature_in_process_write_signs_with_the_callers_key() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(KeyType::Secp256k1),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String age: Int }".into(),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            head("created").into(),
            verify("created", Some(1), None).into(),
            verify("created", None, Some(OTHER_KEY)).into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "age": 22 }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            head("updated").into(),
            verify("updated", Some(1), None).into(),
            verify("updated", None, Some(OTHER_KEY)).into(),
        ],
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.signature.embedded_caller_key")]
fn signature_in_process_write_without_identity_signs_with_the_node_key() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(KeyType::Secp256k1),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String age: Int }".into(),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            head("created").into(),
            verify("created", None, None).into(),
            verify("created", Some(1), Some(OTHER_KEY)).into(),
        ],
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}

//! Block verify refuses unsigned chunk create blocks (`defra.chunk.access.verify`).

use integration_test::actions::shared::KeyType;
use integration_test::actions::{Actions, NewNode, Request, VerifyBlockSignature};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;

use super::access_support::{alice_owns_an_article, with_chunk_id, ALICE, BOB};
use super::support::CHUNKS;

fn capture(request: Text, name: &str, path: &str) -> Actions {
    Request {
        request,
        common: Common {
            identity_seed: Some(ALICE),
            capture: captures([(name, path)]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn verify(cid: &str, identity: Option<u64>, error: Option<&str>) -> Actions {
    VerifyBlockSignature {
        cid: Text::reference(cid),
        signer_seed: Some(ALICE),
        expected_error: error.map(str::to_owned),
        common: Common {
            identity_seed: identity,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.access.verify")]
fn block_verify_refuses_unsigned_chunk_create_blocks() {
    let mut actions: Vec<Actions> = vec![NewNode {
        enable_signing: true,
        key_type: Some(KeyType::Secp256k1),
        ..Default::default()
    }
    .into()];
    actions.extend(alice_owns_an_article());
    actions.extend([
        capture(
            format!("query {{ {CHUNKS}(order: {{hash: ASC}}) {{ _docID }} }}").into(),
            "chunk",
            &format!("/data/{CHUNKS}/0/_docID"),
        ),
        capture(
            Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("article"),
                Text::from(r#"", filter: {fieldName: {_eq: "_C"}}) { cid } }"#),
            ]),
            "head",
            "/data/_commits/0/cid",
        ),
        capture(
            with_chunk_id(
                r#"query { _commits(docID: ""#,
                r#"", filter: {fieldName: {_eq: "_C"}}) { cid } }"#,
            ),
            "chunkHead",
            "/data/_commits/0/cid",
        ),
        capture(
            with_chunk_id(
                r#"query { _commits(docID: ""#,
                r#"", filter: {fieldName: {_eq: "text"}}) { cid } }"#,
            ),
            "chunkText",
            "/data/_commits/0/cid",
        ),
        verify("head", Some(ALICE), None),
        verify("head", Some(BOB), Some("missing permission")),
        verify("head", None, Some("missing permission")),
    ]);
    // Create and update chunk blocks are unsigned even on a signing node, so
    // verify refuses them before any content is returned.
    for cid in ["chunkHead", "chunkText"] {
        actions.extend([
            verify(cid, Some(ALICE), Some("block has no signature")),
            verify(cid, Some(BOB), Some("block has no signature")),
            verify(cid, None, Some("block has no signature")),
        ]);
    }
    let mut case = TestCase::from_actions(actions);
    // In process Alice's identity signs her writes; over HTTP and the CLI the
    // node's key does, as in the ACP signature scenarios.
    case.client_types = Some(&[ClientType::InProcess]);
    case.execute();
}

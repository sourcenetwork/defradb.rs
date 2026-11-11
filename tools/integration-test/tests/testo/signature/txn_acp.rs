//! Signature verification inside a transaction decides document access at the
//! transaction's view: a document the transaction created is registered there,
//! before it commits.

use integration_test::actions::shared::KeyType;
use integration_test::actions::{
    AddCollection, AddDACPolicy, AddDoc, NewNode, Request, TxnBegin, TxnDiscard,
    VerifyBlockSignature,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

const POLICY: &str = r#"
name: test
description: a test policy which marks a collection in a database as a resource

resources:
- name: users
  permissions:
  - name: read
    expr: reader
  - name: update
  - name: delete

  relations:
  - name: reader
    types:
    - actor

  - name: admin
    manages:
    - reader
    types:
    - actor
"#;

#[test]
#[mantra_macros::req_test("defra.txn.verify-signature.view")]
fn an_anonymous_caller_cannot_verify_a_block_the_txn_created() {
    // The transaction is begun anonymously, so only an anonymous caller may
    // verify in it. The document identity 1 creates in it is registered to
    // identity 1 there, so it is not public: an ACP check outside the
    // transaction would find no registration and let the anonymous caller in.
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(KeyType::Secp256k1),
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: POLICY.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("policy", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from("type Users @policy(id: \""),
                    Text::reference("policy"),
                    Text::from("\", resource: \"users\") { name: String age: Int }"),
                ]),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddDoc {
                txn: Some(0),
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
            Request {
                txn: Some(0),
                request: r#"query ($docID: [ID!]) { _commits(docID: $docID, filter: {fieldName: {_eq: "_C"}}) { cid } }"#.into(),
                variables: Some(json!({ "docID": [m::r("doc")] })),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("head", "/data/_commits/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                txn: Some(0),
                signer_seed: Some(1),
                cid: Text::reference("head"),
                expected_error: Some("missing permission".into()),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

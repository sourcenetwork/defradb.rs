//! Generated from the conformance scenarios of tests/integration/txn/verify_signature_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, NodeOpenSigningEd25519, Request, TxnBegin, TxnDiscard,
    VerifyBlockSignature,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.txn.verify-signature")]
fn txn_verify_block_signature_inside_txn_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenSigningEd25519 {
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddCollection {
                txn: Some(0),
                sdl: r#"
                    type Users {
                        name: String
                        age: Int 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                txn: Some(0),
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common { capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"query ($docID: [ID!]) { _commits(docID: $docID, filter: {fieldName: {_eq: "_C"}}, order: {height: ASC}) { cid } }"#.into(),
                variables: Some(json!({ "docID": [m::r("doc0")] })),
                common: Common { capture: captures([("composite0", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                txn: Some(0),
                signer_node: "db".into(),
                cid: Text::reference("composite0"),
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

#[test]
#[mantra_macros::req_test("go.txn.verify-signature")]
fn txn_verify_block_signature_outside_txn_fails() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenSigningEd25519 {
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddCollection {
                txn: Some(0),
                sdl: r#"
                    type Users {
                        name: String
                        age: Int 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                txn: Some(0),
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common { capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"query ($docID: [ID!]) { _commits(docID: $docID, filter: {fieldName: {_eq: "_C"}}, order: {height: ASC}) { cid } }"#.into(),
                variables: Some(json!({ "docID": [m::r("doc0")] })),
                common: Common { capture: captures([("composite0", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                txn: Some(1),
                signer_node: "db".into(),
                cid: Text::reference("composite0"),
                expected_error: Some("ipld: could not find".into()),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 1,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

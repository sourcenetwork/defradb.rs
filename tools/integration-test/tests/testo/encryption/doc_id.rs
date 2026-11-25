//! The docID of an encrypted document, measured against Go.

use integration_test::actions::{AddCollection, AddDoc, DeleteDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

/// Go's docID and commit CIDs for `john21Doc` created with `encrypt: true`,
/// measured from a Go integration test at `GO_COMPAT_COMMIT` (307364d9d),
/// which encrypts with Go's test key and nonce as the testo nodes do. Go's
/// `_C` block links no `Encryption` block (`internal/core/block/store.go`
/// `addDelta` encrypts only field deltas), so its CID, and the docID derived
/// from it, match only when Rust's composite carries no encryption link.
#[test]
#[mantra_macros::req_test("go.encryption.doc.doc_id")]
fn doc_encryption_doc_id_matches_go() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int @crdt(type: lww)
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{
                        "_docID": "bae-e1226eb9-e7dc-5185-986b-6236af58b1ed",
                        "name": "John",
                        "age": 21,
                    }],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                            docID
                            fieldName
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        {
                            "cid": "bafyreigoznnwot6zksvrkculblsap5qxh4usr36ffgft3fc2bbplvwrpva",
                            "docID": "bae-e1226eb9-e7dc-5185-986b-6236af58b1ed",
                            "fieldName": "age",
                        },
                        {
                            "cid": "bafyreia6xo3uz5itn372e2rklqoyzkc4p2zirdzvzuv2kvetogymea2rvy",
                            "docID": "bae-e1226eb9-e7dc-5185-986b-6236af58b1ed",
                            "fieldName": "name",
                        },
                        {
                            "cid": "bafyreiagbtfpc23gmdiycsdfkzcks544xe7ud4i7joxwj47bdmmwhdlclm",
                            "docID": "bae-e1226eb9-e7dc-5185-986b-6236af58b1ed",
                            "fieldName": "_C",
                        },
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

/// Go's `_C` commit CIDs for `john21Doc` created with `encrypt: true`, then
/// updated (`age` to 22), then deleted, measured from a Go integration test
/// at `GO_COMPAT_COMMIT` (307364d9d). An update's and a delete's composite
/// link no `Encryption` block in Go, so each CID matches only when Rust's
/// update and delete composites carry no encryption link either.
#[test]
#[mantra_macros::req_test("go.encryption.doc.doc_id")]
fn doc_encryption_update_and_delete_composites_match_go() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int @crdt(type: lww)
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(filter: {fieldName: {_eq: "_C"}}) {
                            cid
                            height
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        {
                            "cid": "bafyreiai273itfvct3f5hpgmomqmmg5smge7wkjnj7jv4s26shxf7kses4",
                            "height": 2,
                        },
                        {
                            "cid": "bafyreiagbtfpc23gmdiycsdfkzcks544xe7ud4i7joxwj47bdmmwhdlclm",
                            "height": 1,
                        },
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(filter: {fieldName: {_eq: "_C"}}) {
                            cid
                            height
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        {
                            "cid": "bafyreiahuiesmme7ahcnw4skm2sx2r4gvym6hly7wws5izoipprvywki6m",
                            "height": 3,
                        },
                        {
                            "cid": "bafyreiai273itfvct3f5hpgmomqmmg5smge7wkjnj7jv4s26shxf7kses4",
                            "height": 2,
                        },
                        {
                            "cid": "bafyreiagbtfpc23gmdiycsdfkzcks544xe7ud4i7joxwj47bdmmwhdlclm",
                            "height": 1,
                        },
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

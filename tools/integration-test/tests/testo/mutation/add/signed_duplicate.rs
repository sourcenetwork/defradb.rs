//! A signed create whose document ID is already stored is refused with Go's
//! text (`defra.document.id.collision`) and a stable code
//! (`defra.document.id.collision_code`). The code is checked on in-process and
//! rust-http; cli and go-http print Go's text only.

use integration_test::actions::shared::KeyType;
use integration_test::actions::{
    AddCollection, AddDoc, AddDocClientSigned, DeleteDoc, NewNode, Request,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use query::executor::{DOCUMENT_ALREADY_EXISTS_ERROR_CODE, DOCUMENT_DELETED_ERROR_CODE};
use serde_json::json;

const USERS: &str = r#"
    type Users {
        name: String
        age: Int
    }
"#;

const EXISTS: &str = "a document with the given ID already exists. DocID: ";
const DELETED: &str = "a document with the given ID has been deleted. DocID: ";

fn signing_node(key_type: Option<KeyType>) -> NewNode {
    NewNode {
        enable_signing: true,
        key_type,
        ..Default::default()
    }
}

fn users() -> AddCollection {
    AddCollection {
        sdl: USERS.into(),
        ..Default::default()
    }
}

fn john(capture: Option<&'static str>) -> AddDoc {
    AddDoc {
        collection: "Users".into(),
        doc_map: Some(json!({ "name": "John", "age": 27 })),
        common: Common {
            capture: capture
                .map(|name| captures([(name, "/docID")]))
                .unwrap_or_default(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.document.id.collision_code")]
fn a_node_signed_duplicate_create_is_refused_with_its_code() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            signing_node(None).into(),
            users().into(),
            john(Some("doc")).into(),
            AddDoc {
                expected_error: Some(EXISTS.into()),
                expected_error_code: Some(DOCUMENT_ALREADY_EXISTS_ERROR_CODE.into()),
                ..john(None)
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.document.id.collision")]
fn a_secp256k1_signed_duplicate_create_is_refused_with_go_text() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            signing_node(Some(KeyType::Secp256k1)).into(),
            users().into(),
            john(Some("doc")).into(),
            AddDoc {
                expected_error: Some(EXISTS.into()),
                ..john(None)
            }
            .into(),
            Request {
                request: "query { Users { _docID name } }".into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc"),"name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.document.id.collision_code")]
fn a_request_duplicate_create_is_refused_with_its_code() {
    let create = r#"mutation { add_Users(input: { name: "John", age: 27 }) { _docID } }"#;
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            signing_node(None).into(),
            users().into(),
            Request {
                request: create.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: create.into(),
                expected_error: Some(EXISTS.into()),
                expected_error_code: Some(DOCUMENT_ALREADY_EXISTS_ERROR_CODE.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.document.id.collision_code")]
fn a_deleted_document_created_again_is_refused_as_deleted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            signing_node(None).into(),
            users().into(),
            john(Some("doc")).into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                expected_error: Some(DELETED.into()),
                expected_error_code: Some(DOCUMENT_DELETED_ERROR_CODE.into()),
                ..john(None)
            }
            .into(),
            Request {
                request: "query { Users { _docID } }".into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.document.id.collision_code")]
fn an_upsert_on_a_deleted_document_is_refused_as_deleted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            signing_node(None).into(),
            users().into(),
            john(Some("doc")).into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                doc_id: Some(Text::reference("doc")),
                doc_map: Some(json!({ "name": "Fred" })),
                upsert: true,
                expected_error: Some(DELETED.into()),
                expected_error_code: Some(DOCUMENT_DELETED_ERROR_CODE.into()),
                ..john(None)
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.document.id.collision")]
fn a_client_signed_duplicate_create_is_refused() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode::default().into(),
            users().into(),
            AddDocClientSigned {
                collection: "Users".into(),
                doc_map: json!({ "name": "John", "age": 27 }),
                signer_seed: 1,
                ..Default::default()
            }
            .into(),
            AddDocClientSigned {
                collection: "Users".into(),
                doc_map: json!({ "name": "John", "age": 27 }),
                signer_seed: 1,
                expected_error: Some(EXISTS.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

//! Which encryption an update without config gives each field it writes.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const USERS: &str = r#"
    type Users {
        name: String
        age: Int @crdt(type: lww)
        verified: Boolean
    }
"#;

fn add_users() -> AddCollection {
    AddCollection {
        sdl: USERS.into(),
        ..Default::default()
    }
}

fn create(doc: Value, encrypt_doc: bool, encrypted_fields: &[&str]) -> AddDoc {
    AddDoc {
        collection: "Users".into(),
        doc_map: Some(doc),
        is_doc_encrypted: Some(encrypt_doc),
        encrypted_fields: Some(encrypted_fields.iter().map(|f| f.to_string()).collect()),
        common: Common {
            capture: captures([("doc0", "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn update(doc: Value) -> UpdateDoc {
    UpdateDoc {
        collection: "Users".into(),
        doc_id: Text::reference("doc0"),
        doc_map: doc,
        expected_data: json!({}),
        ..Default::default()
    }
}

fn field_deltas(field: &str, expected: Value) -> Request {
    Request {
        request: format!(
            r#"
                query {{
                    _commits(filter: {{fieldName: {{_eq: "{field}"}}}}) {{
                        delta
                    }}
                }}
            "#
        )
        .into(),
        expected_data: json!({ "_commits": expected }),
        ..Default::default()
    }
}

/// The document reads back decrypted, whatever each field's encryption.
fn reads_back(expected: Value) -> Request {
    Request {
        request: r#"
            query {
                Users {
                    name
                    age
                    verified
                }
            }
        "#
        .into(),
        expected_data: json!({ "Users": [expected] }),
        ..Default::default()
    }
}

/// Case (i): every field of an `encrypt: true` document is encrypted, so a
/// field an update writes first gets a fresh document-scope key. Go writes it
/// in plaintext (`internal/core/block/store.go`: a field with no heads has
/// nothing to inherit), so this case is Rust-only.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn doc_encryption_new_field_on_encrypted_doc_is_encrypted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_users().into(),
            create(json!({ "age": 21 }), true, &[]).into(),
            update(json!({ "name": "John" })).into(),
            field_deltas(
                "name",
                json!([{ "delta": m::not_plain_cbor_string(json!("John")) }]),
            )
            .into(),
            reads_back(json!({ "name": "John", "age": 21, "verified": null })).into(),
        ],
        ..Default::default()
    }
    .execute();
}

/// Case (ii): one field is plaintext, so the document is not encrypted as a
/// whole and a field an update writes first stays plaintext, as in Go.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn doc_encryption_new_field_on_partly_encrypted_doc_is_plaintext() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_users().into(),
            create(json!({ "name": "John", "age": 21 }), false, &["age"]).into(),
            update(json!({ "verified": true })).into(),
            field_deltas("verified", json!([{ "delta": "9Q==" }])).into(),
            field_deltas("age", json!([{ "delta": m::length(40) }])).into(),
            reads_back(json!({ "name": "John", "age": 21, "verified": true })).into(),
        ],
        ..Default::default()
    }
    .execute();
}

/// Case (iii), the accepted cost: every field written so far was listed in
/// `encryptedFields`, so the document counts as encrypted as a whole and a
/// field an update writes first gets a fresh document-scope key. Go writes it
/// in plaintext, so this case is Rust-only.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn doc_encryption_new_field_when_every_written_field_was_listed_is_encrypted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_users().into(),
            create(json!({ "age": 21 }), false, &["age"]).into(),
            update(json!({ "name": "John" })).into(),
            field_deltas(
                "name",
                json!([{ "delta": m::not_plain_cbor_string(json!("John")) }]),
            )
            .into(),
            reads_back(json!({ "name": "John", "age": 21, "verified": null })).into(),
        ],
        ..Default::default()
    }
    .execute();
}

/// Case (iv): nothing is encrypted, so a field an update writes first stays
/// plaintext, as in Go.
#[test]
#[mantra_macros::req_test("go.encryption.doc.new_field")]
fn doc_encryption_new_field_on_unencrypted_doc_is_plaintext() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_users().into(),
            create(json!({ "age": 21 }), false, &[]).into(),
            update(json!({ "name": "John" })).into(),
            field_deltas("name", json!([{ "delta": "ZEpvaG4=" }])).into(),
            reads_back(json!({ "name": "John", "age": 21, "verified": null })).into(),
        ],
        ..Default::default()
    }
    .execute();
}

/// With `encryptedFields: [age]`, an update without config of `name`, whose
/// heads are plaintext, writes plaintext, and `age` stays encrypted, as in Go.
#[test]
#[mantra_macros::req_test("go.encryption.doc.field_level_update")]
fn doc_encryption_field_level_update_keeps_unlisted_field_plaintext() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            add_users().into(),
            create(json!({ "name": "John", "age": 21 }), false, &["age"]).into(),
            update(json!({ "name": "Johnny" })).into(),
            field_deltas(
                "name",
                m::unordered(json!([{ "delta": "ZEpvaG4=" }, { "delta": "ZkpvaG5ueQ==" }])),
            )
            .into(),
            field_deltas("age", json!([{ "delta": m::length(40) }])).into(),
            reads_back(json!({ "name": "Johnny", "age": 21, "verified": null })).into(),
        ],
        ..Default::default()
    }
    .execute();
}

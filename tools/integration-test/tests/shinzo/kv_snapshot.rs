//! KV snapshot import (`go.shinzo.kv_snapshot`): a Rust node imports what an
//! old Go Shinzo node exported. The snapshots are fixtures in
//! `tests/shinzo/fixtures/`, exported once by Go Shinzo shinzo-develop
//! 2a899b860 with `tools/go-shinzo-kv/fixtures.sh`, which says what each one
//! holds. No test starts a Go node. `users-signed.kv` and
//! `users-encrypted.kv` were made the same way on 2026-11-05, by `kvsnap add`
//! with signing on, as a signing Go Shinzo node writes its blocks.
//!
//! The Go node made Alice (40) and Bob (7) in `User`, and a Log at
//! blockNumber 5. A second Rust node makes the same documents here: the
//! imported ones must carry the DocIDs it gets.

use integration_test::actions::{
    Actions, AddCollection, AddDoc, CutoffSpec, KvImport, NewNode, Request, RetentionSpec,
};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const IMPORTING: &str = "db0";
const MAKER: &str = "db1";
const USERS: &str = "type User { name: String @index age: Int }";
const LOGS: &str = "type Log { address: String blockNumber: Int }";
const LOG_ID: &str = "bafyreibjraiyakk3irmdr7nl6ijzamzemaebryyxsbfpdexz3dhmglxtae";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn rust(name: &str) -> NewNode {
    NewNode {
        rust: true,
        common: on(name),
        ..Default::default()
    }
}

fn collection(node: &str, sdl: &str) -> Actions {
    AddCollection {
        sdl: sdl.into(),
        common: on(node),
        ..Default::default()
    }
    .into()
}

fn add(node: &str, collection: &str, doc: Value, capture: &str) -> Actions {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(doc),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

fn import(file: &str) -> KvImport {
    KvImport {
        file: file.into(),
        common: on(IMPORTING),
        ..Default::default()
    }
}

fn query(request: &str, expected: Value) -> Actions {
    Request {
        request: request.into(),
        expected_data: expected,
        common: on(IMPORTING),
        ..Default::default()
    }
    .into()
}

fn doc_id(capture: &str) -> Value {
    json!({ "$ref": capture })
}

/// The users the importing node holds, by name, with their DocIDs.
fn users(expected: Value) -> Actions {
    query(
        "query { User(order: {name: ASC}) { _docID name age } }",
        json!({ "User": expected }),
    )
}

/// Alice and Bob made on the second node, for their DocIDs.
fn made_users() -> Vec<Actions> {
    vec![
        rust(IMPORTING).into(),
        rust(MAKER).into(),
        collection(MAKER, USERS),
        add(
            MAKER,
            "User",
            json!({ "name": "Alice", "age": 40 }),
            "alice",
        ),
        add(MAKER, "User", json!({ "name": "Bob", "age": 7 }), "bob"),
        collection(IMPORTING, USERS),
    ]
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.format")]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.import")]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.indexes")]
fn kv_a_go_snapshot_imports_with_its_docids_and_indexes() {
    let both = json!([
        { "_docID": doc_id("alice"), "name": "Alice", "age": 40 },
        { "_docID": doc_id("bob"), "name": "Bob", "age": 7 },
    ]);
    let mut actions = made_users();
    actions.extend([
        import("users.kv").into(),
        users(both.clone()),
        query(
            r#"query { User(filter: {name: {_eq: "Bob"}}) { age } }"#,
            json!({ "User": [{ "age": 7 }] }),
        ),
        // A second import of the same snapshot changes nothing.
        import("users.kv").into(),
        users(both),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.import")]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.history")]
fn kv_a_go_snapshot_of_an_updated_document_has_no_history() {
    // `users-updated.kv`: Alice (short ID 1) was updated to 41, so the
    // snapshot holds only her newest version; Bob (short ID 2) is whole.
    let mut actions = made_users();
    actions.extend([
        KvImport {
            expected_error: Some(
                "the documents with short IDs [1]: it has no create block for them, and a DocID \
                 is derived from the create block"
                    .into(),
            ),
            ..import("users-updated.kv")
        }
        .into(),
        users(json!([{ "_docID": doc_id("bob"), "name": "Bob", "age": 7 }])),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.import")]
fn kv_import_refuses_a_field_this_node_lacks() {
    TestCase::from_actions(vec![
        rust(IMPORTING).into(),
        collection(IMPORTING, USERS),
        KvImport {
            expected_error: Some("the field mapping names field email".into()),
            ..import("users-email.kv")
        }
        .into(),
        users(json!([])),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.import")]
fn kv_a_datastore_only_snapshot_is_refused() {
    TestCase::from_actions(vec![
        rust(IMPORTING).into(),
        collection(IMPORTING, USERS),
        KvImport {
            expected_error: Some("a datastore-only snapshot carries no DocIDs".into()),
            ..import("users-datastore.kv")
        }
        .into(),
    ])
    .execute();
}

/// `users-signed.kv`: Alice (40) and Bob (7), added in-process by a fresh
/// Ed25519 identity with signing on (`kvsnap add`, in `fixtures.sh`), as a
/// signing Go Shinzo node writes them. Every block links a signature block the
/// export left out. The DocIDs are the ones the Go node printed: a signed
/// first version's CID names its signature, so they are the signer's own.
#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.signatures")]
fn kv_a_signed_go_snapshot_imports_without_its_signature_blocks() {
    TestCase::from_actions(vec![
        rust(IMPORTING).into(),
        collection(IMPORTING, USERS),
        import("users-signed.kv").into(),
        users(json!([
            { "_docID": "bae-a3782ddd-fd6e-579a-addb-e18f30bc353e", "name": "Alice", "age": 40 },
            { "_docID": "bae-b7c5a503-b53b-5323-9923-269efd858a37", "name": "Bob", "age": 7 },
        ])),
        query(
            r#"query { User(filter: {name: {_eq: "Bob"}}) { age } }"#,
            json!({ "User": [{ "age": 7 }] }),
        ),
    ])
    .execute();
}

/// `users-encrypted.kv`: Alice (40), added like `users-signed.kv` but
/// encrypted (`kvsnap add --encrypt`). Her blocks link an encryption block
/// the export left out, and the key stays on the Go node, so her values
/// cannot be read: the import names her and stores nothing of her.
#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.encryption")]
fn kv_an_encrypted_go_document_is_reported_not_stored() {
    TestCase::from_actions(vec![
        rust(IMPORTING).into(),
        collection(IMPORTING, USERS),
        KvImport {
            expected_error: Some(
                r#"documents ["bae-cce93ba4-d0ce-52ab-91d0-d74fcaa6c3f5"] are encrypted"#.into(),
            ),
            ..import("users-encrypted.kv")
        }
        .into(),
        users(json!([])),
    ])
    .execute();
}

fn logs(expected: Value) -> Actions {
    query(
        "query { Log { address blockNumber } }",
        json!({ "Log": expected }),
    )
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.restore")]
fn kv_an_import_restores_what_the_floor_and_the_filter_refuse_from_peers() {
    TestCase::from_actions(vec![
        NewNode {
            retention: Some(RetentionSpec {
                collection_id: LOG_ID.into(),
                field: "blockNumber".into(),
                floor: 100,
            }),
            refuse_at_or_below: Some(CutoffSpec {
                field: "blockNumber".into(),
                cutoff: 100,
            }),
            ..rust(IMPORTING)
        }
        .into(),
        collection(IMPORTING, LOGS),
        import("logs.kv").into(),
        logs(json!([{ "address": "a5", "blockNumber": 5 }])),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.batches")]
fn kv_live_writes_during_an_import_are_kept() {
    TestCase::from_actions(vec![
        rust(IMPORTING).into(),
        collection(IMPORTING, LOGS),
        import("logs.kv").into(),
        query(
            r#"mutation { update_Log(filter: {address: {_eq: "a5"}}, input: {blockNumber: 7}) { blockNumber } }"#,
            json!({ "update_Log": [{ "blockNumber": 7 }] }),
        ),
        // Importing the older snapshot again keeps the newer write.
        import("logs.kv").into(),
        logs(json!([{ "address": "a5", "blockNumber": 7 }])),
    ])
    .execute();
}

//! KV snapshot export and import (`go.shinzo.kv_snapshot`). A `shinzo_kv_`
//! test exchanges snapshots with a Go Shinzo node (`DEFRA_GO_SHINZO_BINARY`,
//! run by `kvsnap` at `DEFRA_GO_SHINZO_KVSNAP`); the Rust node speaks Go
//! Shinzo's wire, so it stays a Rust node under every client. The others run
//! between Rust nodes.

use integration_test::actions::{
    Actions, AddCollection, AddDoc, KvCompare, KvExport, KvImport, NewNode, Request, UpdateDoc,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const RUST: &str = "db0";
const GO: &str = "db1";
const USERS: &str = "type User { name: String @index age: Int }";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn rust(name: &str) -> Actions {
    NewNode {
        shinzo_wire: true,
        common: on(name),
        ..Default::default()
    }
    .into()
}

fn go(name: &str) -> Actions {
    NewNode {
        go_shinzo: true,
        database_dir: Some("".into()),
        common: on(name),
        ..Default::default()
    }
    .into()
}

fn collection(node: &str, sdl: &str) -> Actions {
    AddCollection {
        sdl: sdl.into(),
        common: on(node),
        ..Default::default()
    }
    .into()
}

fn add(node: &str, name: &str, age: i64, capture: &str) -> Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name, "age": age })),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

fn set_age(node: &str, doc: &str, age: i64) -> Actions {
    UpdateDoc {
        collection: "User".into(),
        doc_id: Text::reference(doc),
        doc_map: json!({ "age": age }),
        common: on(node),
        ..Default::default()
    }
    .into()
}

fn export(node: &str, docs: &[&str], file: &str) -> KvExport {
    KvExport {
        collection: "User".into(),
        doc_ids: docs.iter().map(|doc| Text::reference(doc)).collect(),
        file: file.into(),
        common: on(node),
        ..Default::default()
    }
}

fn import(node: &str, file: &str) -> KvImport {
    KvImport {
        file: file.into(),
        common: on(node),
        ..Default::default()
    }
}

fn query(node: &str, request: &str, expected: Value) -> Actions {
    Request {
        request: request.into(),
        expected_data: expected,
        common: on(node),
        ..Default::default()
    }
    .into()
}

fn doc_id(capture: &str) -> Value {
    json!({ "$ref": capture })
}

/// The users `node` holds, by name, with their DocIDs.
fn users(node: &str, expected: Value) -> Actions {
    query(
        node,
        "query { User(order: {name: ASC}) { _docID name age } }",
        json!({ "User": expected }),
    )
}

/// The index on `name` finds `name` with `age`.
fn indexed(node: &str, name: &str, age: i64) -> Actions {
    query(
        node,
        &format!(r#"query {{ User(filter: {{name: {{_eq: "{name}"}}}}) {{ age }} }}"#),
        json!({ "User": [{ "age": age }] }),
    )
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.format")]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.export")]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.field_mapping")]
fn shinzo_kv_go_and_rust_write_the_same_snapshot() {
    let mut actions = vec![rust(RUST), go(GO)];
    for node in [RUST, GO] {
        actions.extend([
            collection(node, USERS),
            add(node, "Alice", 40, &format!("alice-{node}")),
            add(node, "Bob", 7, &format!("bob-{node}")),
            set_age(node, &format!("alice-{node}"), 41),
        ]);
    }
    actions.extend([
        export(GO, &["alice-db1", "bob-db1"], "go").into(),
        KvExport {
            without_history: true,
            ..export(RUST, &["alice-db0", "bob-db0"], "rust")
        }
        .into(),
        KvCompare {
            left: "go".into(),
            right: "rust".into(),
            ..Default::default()
        }
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.import")]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.indexes")]
fn shinzo_kv_a_go_snapshot_imports_into_rust() {
    TestCase::from_actions(vec![
        rust(RUST),
        go(GO),
        collection(GO, USERS),
        add(GO, "Alice", 40, "alice"),
        add(GO, "Bob", 7, "bob"),
        export(GO, &["alice", "bob"], "go").into(),
        collection(RUST, USERS),
        import(RUST, "go").into(),
        users(
            RUST,
            json!([
                { "_docID": doc_id("alice"), "name": "Alice", "age": 40 },
                { "_docID": doc_id("bob"), "name": "Bob", "age": 7 },
            ]),
        ),
        indexed(RUST, "Bob", 7),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.import")]
fn shinzo_kv_a_go_snapshot_of_an_updated_document_has_no_history() {
    TestCase::from_actions(vec![
        rust(RUST),
        go(GO),
        collection(GO, USERS),
        add(GO, "Alice", 40, "alice"),
        add(GO, "Bob", 7, "bob"),
        set_age(GO, "alice", 41),
        export(GO, &["alice", "bob"], "go").into(),
        collection(RUST, USERS),
        KvImport {
            expected_error: Some("export them with history".into()),
            ..import(RUST, "go")
        }
        .into(),
        users(
            RUST,
            json!([{ "_docID": doc_id("bob"), "name": "Bob", "age": 7 }]),
        ),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.import")]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.indexes")]
fn shinzo_kv_a_rust_snapshot_imports_into_a_fresh_go_node() {
    TestCase::from_actions(vec![
        rust(RUST),
        go(GO),
        collection(RUST, USERS),
        add(RUST, "Alice", 40, "alice"),
        add(RUST, "Bob", 7, "bob"),
        set_age(RUST, "alice", 41),
        export(RUST, &[], "rust").into(),
        collection(GO, USERS),
        KvImport {
            rebuild_indexes: Some("User".into()),
            ..import(GO, "rust")
        }
        .into(),
        // Go's import writes no DocID mappings, so its documents read back
        // with no DocID (`go.shinzo.kv_snapshot.import`'s Go-Bug-Suspect).
        users(
            GO,
            json!([
                { "_docID": "", "name": "Alice", "age": 41 },
                { "_docID": "", "name": "Bob", "age": 7 },
            ]),
        ),
        indexed(GO, "Alice", 41),
    ])
    .execute();
}

fn two_rust_nodes() -> Vec<Actions> {
    vec![
        NewNode {
            common: on(RUST),
            ..Default::default()
        }
        .into(),
        NewNode {
            common: on(GO),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.import")]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.indexes")]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.batches")]
fn kv_snapshot_round_trips_between_rust_nodes() {
    let target = GO;
    let mut actions = two_rust_nodes();
    actions.extend([
        collection(RUST, USERS),
        add(RUST, "Alice", 40, "alice"),
        add(RUST, "Bob", 7, "bob"),
        set_age(RUST, "alice", 41),
        export(RUST, &["alice", "bob"], "rust").into(),
        collection(target, USERS),
        import(target, "rust").into(),
    ]);
    let expected = json!([
        { "_docID": doc_id("alice"), "name": "Alice", "age": 41 },
        { "_docID": doc_id("bob"), "name": "Bob", "age": 7 },
    ]);
    actions.extend([
        users(target, expected.clone()),
        indexed(target, "Alice", 41),
        // A second import of the same snapshot changes nothing.
        import(target, "rust").into(),
        users(target, expected),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.export")]
fn kv_export_without_doc_ids_exports_every_document() {
    let mut actions = two_rust_nodes();
    actions.extend([
        collection(RUST, USERS),
        add(RUST, "Alice", 40, "alice"),
        add(RUST, "Bob", 7, "bob"),
        export(RUST, &[], "rust").into(),
        collection(GO, USERS),
        import(GO, "rust").into(),
        users(
            GO,
            json!([
                { "_docID": doc_id("alice"), "name": "Alice", "age": 40 },
                { "_docID": doc_id("bob"), "name": "Bob", "age": 7 },
            ]),
        ),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.import")]
fn kv_import_refuses_a_field_this_node_lacks() {
    let mut actions = two_rust_nodes();
    actions.extend([
        collection(
            RUST,
            "type User { name: String @index age: Int email: String }",
        ),
        add(RUST, "Alice", 40, "alice"),
        export(RUST, &["alice"], "rust").into(),
        collection(GO, USERS),
        KvImport {
            expected_error: Some("the field mapping names field email".into()),
            ..import(GO, "rust")
        }
        .into(),
        users(GO, json!([])),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.kv_snapshot.import")]
fn kv_a_datastore_only_snapshot_is_refused() {
    let mut actions = two_rust_nodes();
    actions.extend([
        collection(RUST, USERS),
        add(RUST, "Alice", 40, "alice"),
        KvExport {
            datastore_only: true,
            ..export(RUST, &["alice"], "rust")
        }
        .into(),
        collection(GO, USERS),
        KvImport {
            expected_error: Some("a datastore-only snapshot carries no DocIDs".into()),
            ..import(GO, "rust")
        }
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

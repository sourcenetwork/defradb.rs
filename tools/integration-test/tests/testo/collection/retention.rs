//! History retention (`defra.retention`): `@retention(maxDepth: N)` and the
//! history prune that applies it, in process, over
//! `POST /collections/{name}/history/prune` and through
//! `client collection prune-history`. Go has no history retention, so go-http
//! does not run the prune.

mod access;
mod p2p;

use integration_test::actions::{
    Actions, AddCollection, AddDoc, Close, DeleteDoc, NewNode, PruneHistory, Request, Start,
    UpdateDoc,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const RUST: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

const RETAINED: &str = "type User @retention(maxDepth: 2) { name: String age: Int }";

/// A scenario for the clients that know `@retention`: Go has none.
fn rust_only(actions: Vec<Actions>) -> TestCase {
    TestCase {
        client_types: Some(RUST),
        ..TestCase::from_actions(actions)
    }
}

fn collection(sdl: &str) -> Actions {
    AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    }
    .into()
}

fn add() -> Actions {
    add_as("john", json!({ "name": "John", "age": 21 }))
}

/// A document `doc_map`, captured as `doc`.
fn add_as(doc: &str, doc_map: Value) -> Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(doc_map),
        common: Common {
            capture: captures([(doc, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn add_with_encrypted_name() -> Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": "John", "age": 21 })),
        is_doc_encrypted: Some(false),
        encrypted_fields: Some(vec!["name".into()]),
        common: Common {
            capture: captures([("john", "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn update(age: i64) -> Actions {
    update_of("john", json!({ "age": age }))
}

fn rename(name: &str) -> Actions {
    update_of("john", json!({ "name": name }))
}

fn update_of(doc: &str, doc_map: Value) -> Actions {
    UpdateDoc {
        collection: "User".into(),
        doc_id: Text::reference(doc),
        doc_map,
        expected_data: json!({}),
        ..Default::default()
    }
    .into()
}

fn prune(expected: usize) -> Actions {
    PruneHistory {
        collection: "User".into(),
        expected_count: Some(expected),
        ..Default::default()
    }
    .into()
}

/// The heights of John's commits of `field`, highest first.
fn heights(field: &str, expected: &[u64]) -> Actions {
    heights_of("john", field, expected)
}

/// The heights of `doc`'s commits of `field`, highest first.
fn heights_of(doc: &str, field: &str, expected: &[u64]) -> Actions {
    let rows: Vec<Value> = expected.iter().map(|h| json!({ "height": h })).collect();
    Request {
        request: Text::concat([
            Text::from(r#"query { _commits(docID: ""#),
            Text::reference(doc),
            Text::from(format!(
                r#"", filter: {{fieldName: {{_eq: "{field}"}}}}, order: {{height: DESC}}) {{ height }} }}"#
            )),
        ]),
        expected_data: json!({ "_commits": rows }),
        ..Default::default()
    }
    .into()
}

fn john(age: i64) -> Actions {
    Request {
        request: "query { User { name age } }".into(),
        expected_data: json!({ "User": [{ "name": "John", "age": age }] }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn prune_keeps_the_top_heights_and_the_value() {
    rust_only(vec![
        collection(RETAINED),
        add(),
        update(22),
        update(23),
        update(24),
        prune(2),
        heights("_C", &[4, 3]),
        heights("age", &[4, 3]),
        heights("name", &[1]),
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("john"),
                Text::from(r#"") { height fieldName } }"#),
            ]),
            expected_data: json!({ "_commits": [
                { "fieldName": "age", "height": 4 },
                { "fieldName": "age", "height": 3 },
                { "fieldName": "name", "height": 1 },
                { "fieldName": "_C", "height": 4 },
                { "fieldName": "_C", "height": 3 },
            ] }),
            ..Default::default()
        }
        .into(),
        john(24),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_pruned_document_takes_updates_and_prunes_again() {
    rust_only(vec![
        collection(RETAINED),
        add(),
        update(22),
        update(23),
        prune(1),
        prune(0),
        update(24),
        john(24),
        heights("_C", &[4, 3, 2]),
        prune(1),
        heights("_C", &[4, 3]),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_collection_without_retention_keeps_its_history() {
    rust_only(vec![
        collection("type User { name: String age: Int }"),
        add(),
        update(22),
        update(23),
        prune(0),
        heights("_C", &[3, 2, 1]),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.retention.directive")]
fn retention_needs_a_positive_max_depth() {
    rust_only(vec![AddCollection {
        sdl: "type User @retention(maxDepth: 0) { name: String }".into(),
        expected_error: Some(
            "@retention directive requires a positive integer 'maxDepth' argument".into(),
        ),
        ..Default::default()
    }
    .into()])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.retention.directive")]
fn a_branchable_collection_refuses_retention() {
    rust_only(vec![AddCollection {
        sdl: "type User @branchable @retention(maxDepth: 2) { name: String }".into(),
        expected_error: Some(
            "invalid retention configuration: a branchable collection keeps its whole history. Collection: User"
                .into(),
        ),
        ..Default::default()
    }
    .into()])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_field_head_kept_by_a_prune_goes_once_superseded() {
    rust_only(vec![
        collection(RETAINED),
        add(),
        update(22),
        update(23),
        update(24),
        prune(2),
        heights("name", &[1]),
        rename("Bob"),
        update(25),
        update(26),
        prune(3),
        heights("_C", &[7, 6]),
        heights("name", &[2]),
        heights("age", &[6, 5]),
        Request {
            request: "query { User { name age } }".into(),
            expected_data: json!({ "User": [{ "name": "Bob", "age": 26 }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

/// A field head inherited the encryption block of the older field block the
/// prune deletes, so the head's key must remain when no kept composite links
/// it, including after the KMS key cache is gone with the node.
#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_field_head_keeps_an_inherited_encryption_block() {
    rust_only(vec![
        NewNode {
            database_dir: Some(String::new()),
            ..Default::default()
        }
        .into(),
        collection("type User @retention(maxDepth: 1) { name: String age: Int }"),
        add_with_encrypted_name(),
        rename("Bob"),
        rename("Carol"),
        update(22),
        prune(3),
        heights("name", &[3]),
        heights("age", &[2]),
        Close {
            keep_data: true,
            ..Default::default()
        }
        .into(),
        Start::default().into(),
        rename("Dave"),
        Request {
            request: "query { User { name age } }".into(),
            expected_data: json!({ "User": [{ "name": "Dave", "age": 22 }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_deleted_document_is_pruned_the_same_way() {
    rust_only(vec![
        collection(RETAINED),
        add(),
        update(22),
        update(23),
        DeleteDoc {
            collection: "User".into(),
            doc_id: Text::reference("john"),
            ..Default::default()
        }
        .into(),
        prune(2),
        heights("_C", &[4, 3]),
        prune(0),
        Request {
            request: "query { User { name } }".into(),
            expected_data: json!({ "User": [] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

/// Jane's create writes the same `name` block as John's. John renames and
/// prunes the composite that linked it; Jane still owns it, so it stays, and
/// John's `_commits` still lists it, as a stored commit of his history.
#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_block_another_document_owns_is_kept() {
    rust_only(vec![
        collection(RETAINED),
        add(),
        add_as("jane", json!({ "name": "John", "age": 30 })),
        rename("Bob"),
        update(22),
        update(23),
        prune(2),
        heights("_C", &[4, 3]),
        heights("name", &[2, 1]),
        heights_of("jane", "name", &[1]),
        heights_of("jane", "_C", &[1]),
        Request {
            request: r#"query { User(order: {age: ASC}) { name age } }"#.into(),
            expected_data: json!({ "User": [
                { "name": "Bob", "age": 23 },
                { "name": "John", "age": 30 },
            ] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

/// A version read rebuilds a document from its whole history, which a prune
/// cut: it fails, naming the pruned commit, instead of returning the fields
/// the kept commits happen to hold.
#[test]
#[mantra_macros::req_test("defra.retention.prune")]
fn a_version_read_past_a_prune_fails() {
    rust_only(vec![
        collection(RETAINED),
        add(),
        update(22),
        update(23),
        update(24),
        Request {
            request: "query { User { _version { cid } } }".into(),
            common: Common {
                capture: captures([("head", "/data/User/0/_version/0/cid")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        version_read(
            Some(json!({ "User": [{ "name": "John", "age": 24 }] })),
            None,
        ),
        prune(2),
        john(24),
        version_read(
            None,
            Some("its history was pruned to the collection's @retention depth"),
        ),
    ])
    .execute();
}

/// John read at the captured `head`.
fn version_read(expected: Option<Value>, error: Option<&str>) -> Actions {
    Request {
        request: Text::concat([
            Text::from(r#"query { User(cid: ""#),
            Text::reference("head"),
            Text::from(r#"") { name age } }"#),
        ]),
        expected_data: expected.unwrap_or(Value::Null),
        expected_error: error.map(Into::into),
        ..Default::default()
    }
    .into()
}

//! History retention (`defra.retention`): `@retention(maxDepth: N)` and the
//! history prune that applies it, in process, over
//! `POST /collections/{name}/history/prune` and through
//! `client collection prune-history`. Go has no history retention, so go-http
//! does not run the prune.

use integration_test::actions::{Actions, AddCollection, AddDoc, PruneHistory, Request, UpdateDoc};
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
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": "John", "age": 21 })),
        common: Common {
            capture: captures([("john", "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn update(age: i64) -> Actions {
    UpdateDoc {
        collection: "User".into(),
        doc_id: Text::reference("john"),
        doc_map: json!({ "age": age }),
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
    let rows: Vec<Value> = expected.iter().map(|h| json!({ "height": h })).collect();
    Request {
        request: Text::concat([
            Text::from(r#"query { _commits(docID: ""#),
            Text::reference("john"),
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

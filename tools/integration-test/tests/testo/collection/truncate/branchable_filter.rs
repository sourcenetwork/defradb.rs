//! A filtered truncate of a `@branchable` collection removes the matching
//! documents and keeps their history, which the collection DAG links. Go's
//! parity pin refuses it, so go-http does not run these.

use integration_test::actions::shared::Filter;
use integration_test::actions::{Actions, AddCollection, AddDoc, Request, Truncate};
use integration_test::m;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const RUST_CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

fn users() -> Actions {
    AddCollection {
        sdl: "type Users @branchable { name: String }".into(),
        ..Default::default()
    }
    .into()
}

fn add(name: &str) -> Actions {
    AddDoc {
        collection: "Users".into(),
        doc_map: Some(json!({ "name": name })),
        ..Default::default()
    }
    .into()
}

fn truncate_named(name: &str) -> Actions {
    Truncate {
        collection: "Users".into(),
        filter: Some(Filter {
            kind: "map".into(),
            value: Some(json!({ "name": { "_eq": name } })),
        }),
        ..Default::default()
    }
    .into()
}

fn names(expected: Value) -> Actions {
    Request {
        request: "query { Users(order: {name: ASC}) { name } }".into(),
        expected_data: json!({ "Users": expected }),
        ..Default::default()
    }
    .into()
}

/// Each create writes a field block, a composite block and a collection block.
/// `_commits` lists a document's blocks while the document exists, and every
/// collection block.
fn commit_count(expected: u64) -> Actions {
    Request {
        request: "query { _commits { cid } }".into(),
        expected_data: json!({ "_commits": m::length(expected) }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate.filter_branchable")]
fn filtered_truncate_of_a_branchable_collection_removes_only_the_matches() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            users().into(),
            add("John").into(),
            add("Jane").into(),
            truncate_named("John").into(),
            names(json!([{ "name": "Jane" }])).into(),
        ],
        client_types: Some(RUST_CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate.filter_branchable")]
fn filtered_truncate_of_a_branchable_collection_keeps_its_history() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            users().into(),
            add("John").into(),
            add("Jane").into(),
            commit_count(6).into(),
            truncate_named("John").into(),
            commit_count(4).into(),
        ],
        client_types: Some(RUST_CLIENTS),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate.filter_branchable")]
fn a_document_truncated_from_a_branchable_collection_can_be_added_again() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            users().into(),
            add("John").into(),
            add("Jane").into(),
            truncate_named("John").into(),
            add("John").into(),
            names(json!([{ "name": "Jane" }, { "name": "John" }])).into(),
        ],
        client_types: Some(RUST_CLIENTS),
        ..Default::default()
    }
    .execute();
}

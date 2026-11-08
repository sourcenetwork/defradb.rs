//! A replicator given a filter pushes only the documents the filter matches,
//! and refuses a filter that names an unknown field, a mutable field, or a
//! value of the wrong type. Filtered replication is defradb.rs only, so these
//! tests run only against the Rust node.

use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddReplicator, NewNode, Request, WaitForPeersEvents,
};
use integration_test::scenario::{Common, Eventually};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const SDL: &str =
    "type AgentDoc { agent_did: String @immutable  kind: String @immutable  body: String }";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// Two networked nodes, `db0` and `db1`, both holding `AgentDoc`.
fn nodes() -> Vec<Actions> {
    let mut actions = Vec::new();
    for node in ["db0", "db1"] {
        actions.push(
            NewNode {
                network: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in ["db0", "db1"] {
        actions.push(
            AddCollection {
                sdl: SDL.into(),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in ["db0", "db1"] {
        actions.push(
            WaitForPeersEvents {
                start: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions
}

fn replicator(filter: Value, expected_error: Option<&str>) -> Actions {
    AddReplicator {
        target: "db1".into(),
        collections: vec!["AgentDoc".into()],
        filter,
        expected_error: expected_error.map(str::to_owned),
        common: on("db0"),
        ..Default::default()
    }
    .into()
}

fn doc(agent_did: &str, kind: &str) -> Actions {
    AddDoc {
        collection: "AgentDoc".into(),
        doc_map: Some(json!({ "agent_did": agent_did, "kind": kind, "body": "b" })),
        common: Common {
            retry: true,
            ..on("db0")
        },
        ..Default::default()
    }
    .into()
}

/// `db1` comes to hold exactly `expected`, as `{agent_did, kind}` rows.
fn db1_holds(expected: Value) -> Actions {
    Request {
        request: "query { AgentDoc { agent_did kind } }".into(),
        expected_data: json!({ "AgentDoc": expected }),
        common: Common {
            unordered_documents: vec!["/data/AgentDoc".into()],
            eventually: Some(Eventually::default()),
            ..on("db1")
        },
        ..Default::default()
    }
    .into()
}

fn case(id: &'static str, actions: Vec<Actions>) {
    TestCase {
        id,
        actions: nodes().into_iter().chain(actions).map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

/// The non-matching document is written first, so by the time both matching
/// ones have arrived it would have too, had it been pushed.
#[test]
#[mantra_macros::req_test("defra.replicator_filter.push")]
fn an_in_filter_pushes_only_the_listed_values() {
    case(
        integration_test::scenario_id!(),
        vec![
            replicator(json!({ "agent_did": { "_in": ["did:a", "did:b"] } }), None),
            doc("did:c", "x"),
            doc("did:a", "x"),
            doc("did:b", "x"),
            db1_holds(json!([
                { "agent_did": "did:a", "kind": "x" },
                { "agent_did": "did:b", "kind": "x" },
            ])),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.replicator_filter.push")]
fn a_filter_on_two_fields_pushes_documents_matching_both() {
    case(
        integration_test::scenario_id!(),
        vec![
            replicator(
                json!({ "agent_did": { "_eq": "did:a" }, "kind": { "_eq": "x" } }),
                None,
            ),
            doc("did:a", "y"),
            doc("did:b", "x"),
            doc("did:a", "x"),
            db1_holds(json!([{ "agent_did": "did:a", "kind": "x" }])),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.replicator_filter.validation")]
fn a_filter_the_schema_cannot_satisfy_is_refused() {
    case(
        integration_test::scenario_id!(),
        vec![
            replicator(json!({ "nosuchfield": { "_eq": "x" } }), Some("not found")),
            replicator(
                json!({ "agent_did": { "_in": ["did:a", 42] } }),
                Some("does not match the field type"),
            ),
            replicator(
                json!({ "_or": [
                    { "agent_did": { "_eq": "did:a" } },
                    { "body": { "_eq": "hello" } },
                ] }),
                Some("must be marked @immutable"),
            ),
        ],
    );
}

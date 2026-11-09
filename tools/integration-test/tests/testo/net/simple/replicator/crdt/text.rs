//! Two peers edit one Text field while apart, then replicate to each other:
//! both end at the same text, holding both edits. Each node creates the same
//! document, which content addressing gives the same ID and first block, so
//! the edits diverge from one shared history. Text CRDTs are defradb.rs only,
//! so the test runs only against the Rust node.

use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddReplicator, NewNode, Request, UpdateDoc, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const NODES: [&str; 2] = ["db0", "db1"];

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn update(node: &str, body: &str) -> Actions {
    UpdateDoc {
        collection: "Note".into(),
        doc_id: Text::reference("note"),
        doc_map: json!({ "body": body }),
        common: Common {
            retry: true,
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

fn replicate(from: &str, to: &str) -> Actions {
    AddReplicator {
        target: to.into(),
        collections: vec!["Note".into()],
        common: on(from),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.crdt.text_convergence")]
fn peers_editing_one_text_apart_converge_with_both_edits() {
    let mut actions: Vec<Actions> = Vec::new();
    for node in NODES {
        actions.push(
            NewNode {
                network: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddCollection {
                sdl: "type Note { body: String @crdt(type: text) }".into(),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in NODES {
        actions.push(
            WaitForPeersEvents {
                start: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddDoc {
                collection: "Note".into(),
                doc_map: Some(json!({ "body": "hello world" })),
                common: match node {
                    "db0" => Common {
                        capture: captures([("note", "/docID")]),
                        ..on(node)
                    },
                    _ => on(node),
                },
                ..Default::default()
            }
            .into(),
        );
    }
    actions.extend([
        update("db0", "hello brave world"),
        update("db1", "hello world!"),
        replicate("db0", "db1"),
        replicate("db1", "db0"),
    ]);
    for node in NODES {
        actions.push(
            Request {
                request: "query { Note { body } }".into(),
                expected_data: json!({ "Note": [{ "body": "hello brave world!" }] }),
                common: Common {
                    eventually: Some(Eventually::default()),
                    ..on(node)
                },
                ..Default::default()
            }
            .into(),
        );
    }
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

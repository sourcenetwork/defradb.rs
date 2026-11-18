//! A filtered replicator ships a matching parent with its chunks (`defra.chunk.replicate.filtered`).

use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddReplicator, NewNode, Request, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

use super::support::{body, chunk_texts_are, eventually, on};

// A replicator filter matches immutable fields only.
const SCHEMA: &str = r#"type Article {
    kind: String @immutable
    body: String @chunk(maxChars: 64)
}"#;

fn article(kind: &str, text: &str) -> Actions {
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "kind": kind, "body": text })),
        common: Common {
            retry: true,
            capture: captures([(kind, "/docID")]),
            ..on(Some("db0"))
        },
        ..Default::default()
    }
    .into()
}

fn replicator() -> Actions {
    AddReplicator {
        target: "db1".into(),
        collections: vec!["Article".into()],
        filter: json!({ "kind": { "_eq": "public" } }),
        common: on(Some("db0")),
        ..Default::default()
    }
    .into()
}

fn ships_public_chunks(replicator_first: bool) {
    let (public, private) = (body(3, "rivers"), body(2, "lakes"));
    let mut actions: Vec<Actions> = Vec::new();
    for node in ["db0", "db1"] {
        actions.push(
            NewNode {
                network: true,
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddCollection {
                sdl: SCHEMA.into(),
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            WaitForPeersEvents {
                start: true,
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
    }
    if replicator_first {
        actions.push(replicator());
    }
    actions.extend([article("private", &private), article("public", &public)]);
    if !replicator_first {
        actions.push(replicator());
    }
    actions.extend([
        eventually(
            Request {
                request: "query { Article { kind } }".into(),
                expected_data: json!({ "Article": [{ "kind": "public" }] }),
                common: on(Some("db1")),
                ..Default::default()
            }
            .into(),
        ),
        eventually(chunk_texts_are(&[&public], Some("db1"))),
    ]);
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    // Filtered replication is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_filtered_replicator_ships_the_matching_parents_chunks() {
    ships_public_chunks(true);
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_filtered_replicator_replays_the_matching_parents_chunks() {
    ships_public_chunks(false);
}

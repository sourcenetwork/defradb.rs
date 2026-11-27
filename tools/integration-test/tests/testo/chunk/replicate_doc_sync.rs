//! A document sync brings the parent with its chunks (`defra.chunk.replicate.doc_sync`).

use integration_test::actions::{
    Actions, AddCollection, AdmissionSpec, ConnectPeers, NewNode, Request, SyncDocs,
    WaitForPeersEvents,
};
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

use super::support::{add_article_on, body, chunk_texts_are, eventually, on, SCHEMA};

/// Two connected nodes with the Article collection; `db0` serves at most
/// `limit` documents per doc sync request when one is given.
fn connected(limit: Option<usize>) -> Vec<Actions> {
    let mut actions: Vec<Actions> = Vec::new();
    for node in ["db0", "db1"] {
        let admission = limit.filter(|_| node == "db0").map(|max| AdmissionSpec {
            max_pending_dags: 1000,
            rate_limit_burst: 500,
            rate_limit_rate: 50.0,
            deferral_peer_bytes: None,
            max_doc_sync_request_doc_ids: Some(max),
        });
        actions.push(
            NewNode {
                network: true,
                admission,
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
    actions.push(
        ConnectPeers {
            target: "db1".into(),
            common: on(Some("db0")),
            ..Default::default()
        }
        .into(),
    );
    actions
}

fn synced(text: &str) -> Vec<Actions> {
    vec![
        add_article_on(text, "article", Some("db0")),
        SyncDocs {
            collection: "Article".into(),
            doc_ids: vec![Text::reference("article")],
            concurrency: 1,
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
        eventually(
            Request {
                request: "query { Article { body } }".into(),
                expected_data: json!({ "Article": [{ "body": text }] }),
                common: on(Some("db1")),
                ..Default::default()
            }
            .into(),
        ),
        eventually(chunk_texts_are(&[text], Some("db1"))),
    ]
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.doc_sync")]
fn a_document_sync_brings_the_parents_chunks() {
    let text = body(3, "rivers");
    let mut actions = connected(None);
    actions.extend(synced(&text));
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    // Document sync is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.doc_sync")]
fn an_article_with_more_chunks_than_the_request_limit_syncs() {
    let text = body(12, "rivers");
    let mut actions = connected(Some(10));
    actions.extend(synced(&text));
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    // The limit is set on an in-process node.
    case.client_types = Some(&[ClientType::InProcess]);
    case.execute();
}

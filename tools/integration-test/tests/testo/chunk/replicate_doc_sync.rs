//! A document sync brings the parent with its chunks (`defra.chunk.replicate.doc_sync`).

use integration_test::actions::{
    Actions, AddCollection, ConnectPeers, NewNode, Request, SyncDocs, WaitForPeersEvents,
};
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

use super::support::{add_article_on, body, chunk_texts_are, eventually, on, SCHEMA};

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.doc_sync")]
fn a_document_sync_brings_the_parents_chunks() {
    let text = body(3, "rivers");
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
    actions.extend([
        ConnectPeers {
            target: "db1".into(),
            common: on(Some("db0")),
            ..Default::default()
        }
        .into(),
        add_article_on(&text, "article", Some("db0")),
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
        eventually(chunk_texts_are(&[&text], Some("db1"))),
    ]);
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    // Document sync is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case.execute();
}

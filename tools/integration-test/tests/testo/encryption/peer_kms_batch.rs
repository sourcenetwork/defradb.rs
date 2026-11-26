//! A peer that receives more encrypted documents at once than one pinc topic
//! message holds keys for still gets every key: the KMS splits its key
//! request so each request and its reply fit one 64 KiB topic message.

use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddReplicator, ConnectPeers, NewNode, Request,
    WaitForPeersEvents,
};
use integration_test::m;
use integration_test::scenario::{Common, Eventually};
use integration_test::test_case::TestCase;
use serde_json::json;

/// More keys than the 470 a reply fitting one topic message holds, and more
/// than the 512 the KMS batched before pinc reported its limit.
const DOCS: usize = 600;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn node(name: &str) -> [Actions; 3] {
    [
        NewNode {
            network: true,
            kms: true,
            common: on(name),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: "type Users { name: String age: Int }".into(),
            common: on(name),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on(name),
            ..Default::default()
        }
        .into(),
    ]
}

/// db0 writes `DOCS` encrypted documents, then replicates them to db1 at
/// once, so db1 asks for every key in one batch.
fn replicated() -> Vec<Actions> {
    let mut actions: Vec<Actions> = [node("db0"), node("db1")].into_iter().flatten().collect();
    actions.extend([
        ConnectPeers {
            target: "db0".into(),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            docs: Some(
                (0..DOCS)
                    .map(|i| json!({ "name": format!("user{i}"), "age": i }))
                    .collect(),
            ),
            is_doc_encrypted: Some(true),
            encrypted_fields: Some(Vec::new()),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        AddReplicator {
            target: "db1".into(),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Users { _docID } }".into(),
            expected_data: json!({ "Users": m::length(DOCS as u64) }),
            common: Common {
                // Every key arrives, however long the batches take: runs here
                // took 28 s to over 45 s. With the batch unsplit the keys
                // still arrived, in about 60 s, so the size guard itself is
                // pinned by defra-pinc's topic tests, not by this one.
                eventually: Some(Eventually {
                    timeout_millis: Some(120_000),
                    ..Default::default()
                }),
                ..on("db1")
            },
            ..Default::default()
        }
        .into(),
    ]);
    actions
}

#[test]
#[mantra_macros::req_test("defra.encryption.peer.kms_batch_split")]
fn a_peer_gets_the_keys_of_more_encrypted_documents_than_one_topic_message_holds() {
    TestCase::from_actions(replicated()).execute();
}

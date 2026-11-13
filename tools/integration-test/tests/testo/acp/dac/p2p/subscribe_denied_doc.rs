//! A peer subscribed to a policy collection before its documents exist hears
//! the head of every new document over gossip, but it may only fetch the rest
//! of a document's blocks when its node identity can read that document. The
//! one it was never granted stays off the peer, however long the peer has had
//! to fetch it: Bitswap serves its field blocks only through the ACP gate.

use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDACActorRelationship, AddDACPolicy,
    AddDoc, ConnectPeers, NewNode, Request, Wait, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;
use testo::action::ActionInstance;

const POLICY: &str = r#"
description: A Policy
name: Test Policy
resources:
- name: users
  permissions:
  - expr: reader
    name: read
  - name: update
  - name: delete
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
"#;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn users_collection(node: &str, policy: &str) -> ActionInstance<Actions> {
    AddCollection {
        sdl: Text::concat([
            Text::from("type Users @policy(id: \""),
            Text::reference(policy),
            Text::from("\", resource: \"users\") { name: String age: Int }"),
        ]),
        common: on(node),
        ..Default::default()
    }
    .into()
}

fn add_policy(node: &str, capture: &str) -> ActionInstance<Actions> {
    AddDACPolicy {
        policy: POLICY.into(),
        expected_data: json!({ "policyID": m::nonempty_string() }),
        common: Common {
            identity_seed: Some(2),
            capture: captures([(capture, "/policyID")]),
            ..on(node)
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("go.acp.dac.p2p")]
fn a_subscribed_peer_never_receives_a_document_its_node_cannot_read() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                with_identity: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                with_identity: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            add_policy("db0", "policy0"),
            add_policy("db1", "policy1"),
            users_collection("db0", "policy0"),
            users_collection("db1", "policy1"),
            WaitForPeersEvents {
                start: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            // Owned by identity 3 and never shared: db1 hears its head over
            // the collection topic and must not get its field blocks.
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Denied", "age": 10 })),
                common: Common {
                    identity_seed: Some(3),
                    retry: true,
                    capture: captures([("denied", "/docID")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("denied")),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Granted", "age": 20 })),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
                    capture: captures([("granted", "/docID")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("granted")),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference("granted"),
                relation: "reader".into(),
                target_identity_node: Some("db1".into()),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(2),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("granted")),
                common: Common {
                    capture: captures([("grantedHead", "/cid")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("granted")),
                cid: Some(Text::reference("grantedHead")),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            // A document that never arrives raises no event to wait on. The
            // granted one has synced; the denied one, whose head db1 already
            // holds, gets many times the tens of milliseconds its block fetch
            // takes to arrive, if it ever could.
            Wait {
                millis: Some(1000),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name age } }".into(),
                expected_data: json!({ "Users": [{ "age": 20, "name": "Granted" }] }),
                common: Common {
                    identity_seed: Some(2),
                    ..on("db1")
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

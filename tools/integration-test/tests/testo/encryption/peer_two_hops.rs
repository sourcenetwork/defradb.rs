//! A key two hops away: a node fetches a document's key from the only node
//! that may share it, through a peer that holds the blocks but not the key.

use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDACActorRelationship, AddDACPolicy,
    AddDoc, ConnectPeers, NewNode, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;
use testo::action::ActionInstance;

const POLICY: &str = r#"
name: Test Policy

description: A Policy

resources:
  - name: users
    permissions:
    - name: read
      expr: reader + updater + deleter
    - name: update
      expr: updater
    - name: delete
      expr: deleter
    - name: nothing
      expr: dummy

    relations:
    - name: reader
      types:
      - actor
    - name: updater
      types:
      - actor
    - name: deleter
      types:
      - actor
    - name: admin
      manages:
      - reader
      types:
      - actor
    - name: dummy
      types:
      - actor
"#;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn as_owner(node: &str) -> Common {
    Common {
        identity_seed: Some(2),
        ..on(node)
    }
}

#[test]
#[mantra_macros::req_test("go.encryption.peer.key_fetch")]
fn doc_encryption_peer_fetches_key_two_hops_away_through_a_peer_without_it() {
    let mut actions: Vec<ActionInstance<Actions>> = Vec::new();
    for (seed, node) in [(0, "a"), (1, "b"), (4, "c")] {
        actions.push(
            NewNode {
                node_identity_seed: Some(seed),
                network: true,
                kms: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    // Every node runs its own ACP, so each holds the policy.
    for node in ["a", "b", "c"] {
        let capture = if node == "a" {
            captures([("policy", "/policyID")])
        } else {
            Default::default()
        };
        actions.push(
            AddDACPolicy {
                policy: POLICY.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    capture,
                    ..as_owner(node)
                },
                ..Default::default()
            }
            .into(),
        );
    }
    for node in ["a", "b", "c"] {
        actions.push(
            AddCollection {
                sdl: Text::concat([
                    Text::from(r#"type Users @policy(id: ""#),
                    Text::reference("policy"),
                    Text::from(r#"", resource: "users") { name: String }"#),
                ]),
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            WaitForPeersEvents {
                start: true,
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    // A line: a – b – c, with no a – c connection.
    actions.push(
        ConnectPeers {
            target: "a".into(),
            common: on("b"),
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        ConnectPeers {
            target: "b".into(),
            common: on("c"),
            ..Default::default()
        }
        .into(),
    );
    for node in ["b", "c"] {
        actions.push(
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "Fred" })),
            is_doc_encrypted: Some(true),
            encrypted_fields: Some(vec![]),
            common: Common {
                retry: true,
                capture: captures([("doc", "/docID")]),
                ..as_owner("a")
            },
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        WaitForSync {
            doc_id: Some(Text::reference("doc")),
            common: on("a"),
            ..Default::default()
        }
        .into(),
    );
    // Node c may read; node b may not, so b never holds the key.
    actions.push(
        AddDACActorRelationship {
            collection: "Users".into(),
            doc_id: Text::reference("doc"),
            relation: "reader".into(),
            target_identity_node: Some("c".into()),
            expected_data: json!({ "existedAlready": false }),
            common: as_owner("a"),
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        WaitForSync {
            doc_id: Some(Text::reference("doc")),
            common: Common {
                capture: captures([("head", "/cid")]),
                ..on("a")
            },
            ..Default::default()
        }
        .into(),
    );
    // A node merges an encrypted block only once it holds the key: b, which
    // a refuses, never merges, so c's merge means its key came from a.
    actions.push(
        WaitForSync {
            doc_id: Some(Text::reference("doc")),
            cid: Some(Text::reference("head")),
            expected_data: json!({}),
            common: on("c"),
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        without_db: true,
        ..Default::default()
    }
    .execute();
}

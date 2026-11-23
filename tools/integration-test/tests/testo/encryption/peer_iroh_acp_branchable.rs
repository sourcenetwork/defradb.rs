//! Over iroh, the key holder's KMS releases a policy-bound document's key to
//! the node identity the iroh handshake authenticated, and only when that
//! identity was granted `reader` (`defra.encryption.peer.iroh_grant`,
//! `defra.encryption.peer.iroh_no_grant`). The key holder, db0, has no node
//! identity; the peer, db1, has one its host registered.

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDACActorRelationship, AddDACPolicy,
    AddDoc, ConnectPeers, GetCollections, NewNode, Request, Wait, WaitForBlocks,
    WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

const KEY_HOLDER: &str = "db0";
const PEER: &str = "db1";

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

fn as_owner_on(node: &str) -> Common {
    Common {
        identity_seed: Some(2),
        ..on(node)
    }
}

fn users_sdl(policy: &str) -> Text {
    Text::concat([
        Text::from(r#"type Users @branchable @policy(id: ""#),
        Text::reference(policy),
        Text::from(r#"", resource: "users") { name: String age: Int }"#),
    ])
}

/// One node's private Users collection, subscribed, with a `reader` grant on
/// the collection to the peer's node identity when `grant` is set.
fn private_users_on(node: &str, grant: bool) -> Vec<Actions> {
    let policy = format!("policy_{node}");
    let collection = format!("collection_{node}");
    let mut actions: Vec<Actions> = vec![
        AddDACPolicy {
            policy: POLICY.into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
            common: Common {
                capture: captures([(policy.as_str(), "/policyID")]),
                ..as_owner_on(node)
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: users_sdl(&policy),
            common: as_owner_on(node),
            ..Default::default()
        }
        .into(),
        GetCollections {
            collection_name: Some("Users".into()),
            filtered: true,
            common: Common {
                capture: captures([(collection.as_str(), "/collections/0/CollectionID")]),
                ..on(node)
            },
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on(node),
            ..Default::default()
        }
        .into(),
    ];
    if grant {
        actions.push(
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference(&collection),
                relation: "reader".into(),
                target_identity_node: Some(PEER.into()),
                expected_data: json!({ "existedAlready": false }),
                common: as_owner_on(node),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        AddCollectionSubscription {
            collections: vec!["Users".into()],
            common: on(node),
            ..Default::default()
        }
        .into(),
    );
    actions
}

/// Two connected iroh nodes on the Users topic, and an encrypted document
/// db0 created anonymously, with its head and the collection's head on db0
/// captured as `docHead` and `collectionHead`.
fn encrypted_doc_on_iroh_pair(grant: bool) -> Vec<Actions> {
    let mut actions: Vec<Actions> = vec![
        NewNode {
            network: true,
            iroh: true,
            kms: true,
            without_node_identity: true,
            common: on(KEY_HOLDER),
            ..Default::default()
        }
        .into(),
        NewNode {
            network: true,
            iroh: true,
            kms: true,
            registered_identity: true,
            common: on(PEER),
            ..Default::default()
        }
        .into(),
    ];
    actions.extend(private_users_on(KEY_HOLDER, grant));
    actions.extend(private_users_on(PEER, grant));
    actions.extend([
        ConnectPeers {
            target: KEY_HOLDER.into(),
            common: on(PEER),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            topics: Some(vec![Topic {
                topic: Text::reference("collection_db0"),
                peers: vec![KEY_HOLDER.into()],
            }]),
            event_type: Some(EventType::Joined),
            millis: Some(10_000),
            common: on(PEER),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "Fred", "age": 33 })),
            is_doc_encrypted: Some(true),
            encrypted_fields: Some(vec![]),
            common: Common {
                retry: true,
                capture: captures([("doc", "/docID")]),
                ..on(KEY_HOLDER)
            },
            ..Default::default()
        }
        .into(),
        WaitForSync {
            doc_ids: Some(vec![
                Text::reference("doc"),
                Text::reference("collection_db0"),
            ]),
            common: Common {
                capture: captures([("docHead", "/cids/0"), ("collectionHead", "/cids/1")]),
                ..on(KEY_HOLDER)
            },
            ..Default::default()
        }
        .into(),
    ]);
    actions
}

fn owner_reads_on_peer(users: serde_json::Value) -> Actions {
    Request {
        request: "query { Users { name } }".into(),
        expected_data: json!({ "Users": users }),
        common: as_owner_on(PEER),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.encryption.peer.iroh_grant")]
fn a_granted_iroh_peer_obtains_the_key() {
    let mut actions = encrypted_doc_on_iroh_pair(true);
    actions.extend([
        WaitForSync {
            doc_id: Some(Text::reference("doc")),
            cid: Some(Text::reference("docHead")),
            common: on(PEER),
            ..Default::default()
        }
        .into(),
        WaitForSync {
            doc_id: Some(Text::reference("collection_db0")),
            cid: Some(Text::reference("collectionHead")),
            common: on(PEER),
            ..Default::default()
        }
        .into(),
        owner_reads_on_peer(json!([{ "name": "Fred" }])),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.encryption.peer.iroh_no_grant")]
fn an_ungranted_iroh_peer_receives_the_blocks_but_not_the_key() {
    let mut actions = encrypted_doc_on_iroh_pair(false);
    actions.extend([
        // Without the key nothing on the peer merges, so only its blockstore
        // shows the document replicated.
        WaitForBlocks {
            cids: vec![Text::reference("docHead")],
            common: on(PEER),
            ..Default::default()
        }
        .into(),
        owner_reads_on_peer(json!([])),
        // A granted peer decrypts within a couple of seconds of receiving the
        // blocks, so a key that was going to arrive has arrived by now.
        Wait {
            millis: Some(5_000),
            ..Default::default()
        }
        .into(),
        owner_reads_on_peer(json!([])),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        without_db: true,
        ..Default::default()
    }
    .execute();
}

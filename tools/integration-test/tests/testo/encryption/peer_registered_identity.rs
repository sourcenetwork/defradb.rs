//! A peer whose node identity the embedding host registered in the signing
//! registry, as the FFI's `identity_ptr` does, fetches the key of an encrypted
//! `@branchable` document it was granted from a node that has no node identity
//! of its own, the same as a peer that loaded its identity from a key
//! (`defra.encryption.peer.registered_identity`).

use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDACActorRelationship, AddDACPolicy,
    AddDoc, ConnectPeers, GetCollections, NewNode, Request, WaitForPeersEvents, WaitForSync,
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

/// Each node's half of the setup: the policy, the collection under it, its
/// collection ID, and a `reader` grant on the collection to db1's node
/// identity.
fn private_users_on(node: &str) -> Vec<ActionInstance<Actions>> {
    let policy = format!("policy_{node}");
    let collection = format!("collection_{node}");
    vec![
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
        AddDACActorRelationship {
            collection: "Users".into(),
            doc_id: Text::reference(&collection),
            relation: "reader".into(),
            target_identity_node: Some("db1".into()),
            expected_data: json!({ "existedAlready": false }),
            common: as_owner_on(node),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("defra.encryption.peer.registered_identity")]
fn a_registered_identity_peer_fetches_an_encrypted_doc_from_a_node_with_no_identity() {
    let mut actions = vec![
        NewNode {
            network: true,
            kms: true,
            without_node_identity: true,
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        NewNode {
            network: true,
            kms: true,
            registered_identity: true,
            common: on("db1"),
            ..Default::default()
        }
        .into(),
    ];
    actions.extend(private_users_on("db0"));
    actions.extend(private_users_on("db1"));
    actions.extend([
        ConnectPeers {
            target: "db0".into(),
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
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "Fred", "age": 33 })),
            is_doc_encrypted: Some(true),
            encrypted_fields: Some(vec![]),
            common: Common {
                retry: true,
                capture: captures([("doc", "/docID")]),
                ..on("db0")
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
                ..on("db0")
            },
            ..Default::default()
        }
        .into(),
        WaitForSync {
            doc_id: Some(Text::reference("doc")),
            cid: Some(Text::reference("docHead")),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        WaitForSync {
            doc_id: Some(Text::reference("collection_db0")),
            cid: Some(Text::reference("collectionHead")),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Users { name } }".into(),
            expected_data: json!({ "Users": [{ "name": "Fred" }] }),
            common: as_owner_on("db1"),
            ..Default::default()
        }
        .into(),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        without_db: true,
        ..Default::default()
    }
    .execute();
}

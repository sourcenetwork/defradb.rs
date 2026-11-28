//! A peer's chunk of a protected parent merges only from a signer who may update the parent
//! (`defra.chunk.replicate.protected`).

use integration_test::actions::{
    Actions, AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc, ConnectPeers, NewNode,
    Request, SyncDocs, WaitForPeersEvents,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::ClientType;
use serde_json::{json, Value};

use super::support::{body, eventually, texts, without_default_node, CHUNKS};

const ALICE: u64 = 2;
const BOB: u64 = 3;
const PARAGRAPHS: usize = 3;
const NODES: [&str; 2] = ["db0", "db1"];
/// A node signs a request's blocks with the key of the identity that makes it
/// only when it holds that key, as an in-process node does. A node behind
/// HTTP or the CLI signs with its own key, so Alice's article would be
/// registered to `db0`'s node identity on `db1`, not to Alice.
const CLIENTS: &[ClientType] = &[ClientType::InProcess];

const POLICY: &str = r#"
name: Test Policy
description: A Policy
resources:
- name: users
  permissions:
  - expr: reader + updater
    name: read
  - expr: updater
    name: update
  - expr: deleter
    name: delete
  relations:
  - name: admin
    manages:
    - reader
    - updater
    types:
    - actor
  - name: deleter
    types:
    - actor
  - name: reader
    types:
    - actor
  - name: updater
    types:
    - actor
"#;

fn on(node: &str, identity: Option<u64>) -> Common {
    Common {
        node: Some(node.into()),
        identity_seed: identity,
        ..Default::default()
    }
}

fn schema(node: &str) -> Text {
    Text::concat([
        Text::from(r#"type Article @policy(id: ""#),
        Text::reference(&format!("policy_{node}")),
        Text::from(
            r#"", resource: "users") {
                title: String
                body: String @chunk(maxChars: 64)
            }"#,
        ),
    ])
}

fn grant(node: &str, relation: &str, bob: bool, peer: Option<&str>) -> Actions {
    AddDACActorRelationship {
        collection: "Article".into(),
        doc_id: Text::reference("article"),
        relation: relation.into(),
        target_identity_seed: bob.then_some(BOB),
        target_identity_node: peer.map(str::to_owned),
        expected_data: json!({ "existedAlready": false }),
        common: on(node, Some(ALICE)),
        ..Default::default()
    }
    .into()
}

/// Two nodes with the policy and the protected `Article`, connected, and
/// Alice's article on `db0`, which each node lets the other read.
fn alice_writes_on_db0(encrypted: bool) -> Vec<Actions> {
    let mut actions = Vec::new();
    for (seed, node) in NODES.iter().enumerate() {
        actions.push(
            NewNode {
                node_identity_seed: Some(seed as u64),
                network: true,
                kms: encrypted,
                with_identity: true,
                enable_signing: true,
                common: on(node, None),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in NODES {
        actions.extend([
            AddDACPolicy {
                policy: POLICY.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    capture: captures([(&format!("policy_{node}"), "/policyID")]),
                    ..on(node, Some(ALICE))
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: schema(node),
                common: on(node, None),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on(node, None),
                ..Default::default()
            }
            .into(),
        ]);
    }
    actions.extend([
        ConnectPeers {
            target: "db1".into(),
            common: on("db0", None),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "t", "body": body(PARAGRAPHS, "plans") })),
            is_doc_encrypted: encrypted.then_some(true),
            encrypted_fields: encrypted.then(Vec::new),
            common: Common {
                retry: true,
                capture: captures([("article", "/docID")]),
                ..on("db0", Some(ALICE))
            },
            ..Default::default()
        }
        .into(),
        grant("db0", "reader", false, Some("db1")),
    ]);
    actions
}

/// Captures Alice's chunks on `db0` without syncing the article.
fn capture_chunks() -> Actions {
    Request {
        request: format!("query {{ {CHUNKS}(order: {{hash: ASC}}) {{ _docID }} }}").into(),
        expected_data: json!({ CHUNKS: m::length(PARAGRAPHS as u64) }),
        common: Common {
            capture: (0..PARAGRAPHS)
                .map(|i| (format!("alice{i}"), format!("/data/{CHUNKS}/{i}/_docID")))
                .collect(),
            ..on("db0", Some(ALICE))
        },
        ..Default::default()
    }
    .into()
}

fn alice_chunks() -> Vec<Text> {
    (0..PARAGRAPHS)
        .map(|i| Text::reference(&format!("alice{i}")))
        .collect()
}

fn sync(collection: &str, ids: Vec<Text>, node: &str) -> Actions {
    SyncDocs {
        collection: collection.into(),
        doc_ids: ids,
        concurrency: 1,
        common: on(node, None),
        ..Default::default()
    }
    .into()
}

/// `node` shows Alice exactly the chunks of `text`.
fn alice_sees(node: &str, text: &str) -> Actions {
    let rows: Vec<Value> = texts(text)
        .into_iter()
        .map(|text| json!({ "text": text }))
        .collect();
    let mut common = on(node, Some(ALICE));
    common.unordered = vec![format!("/data/{CHUNKS}")];
    eventually(
        Request {
            request: format!("query {{ Article {{ body }} {CHUNKS} {{ text }} }}").into(),
            expected_data: json!({ "Article": [{ "body": text }], CHUNKS: rows }),
            common,
            ..Default::default()
        }
        .into(),
    )
}

/// Bob, whom only `db1` lets update the article, rewrites it there, and the
/// chunks of his text are named `bob0` to `bob<n-1>`.
fn bob_writes_on_db1(text: &str) -> Vec<Actions> {
    vec![
        grant("db1", "updater", true, None),
        update_article_on("article", text, Some("db1"), BOB),
        Request {
            request: format!("query {{ {CHUNKS} {{ _docID }} }}").into(),
            expected_data: json!({ CHUNKS: m::length(PARAGRAPHS as u64) }),
            common: Common {
                capture: (0..PARAGRAPHS)
                    .map(|i| (format!("bob{i}"), format!("/data/{CHUNKS}/{i}/_docID")))
                    .collect(),
                ..on("db1", Some(BOB))
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn update_article_on(capture: &str, text: &str, node: Option<&str>, identity: u64) -> Actions {
    let mut update = super::support::update_article_on(capture, text, node);
    if let Actions::Request(request) = &mut update {
        request.common.identity_seed = Some(identity);
        request.expected_data = json!({ "update_Article": m::length(1) });
    }
    update
}

/// `db0` holds no history of Bob's chunk `bob<i>`, which its current text uses.
fn db0_has_no_commits_of_bob(i: usize) -> Actions {
    Request {
        request: Text::concat([
            Text::from(r#"query { _commits(docID: ""#),
            Text::reference(&format!("bob{i}")),
            Text::from(r#"") { cid } }"#),
        ]),
        expected_data: json!({ "_commits": [] }),
        common: on("db0", Some(ALICE)),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.protected")]
fn a_peer_chunk_merges_only_from_a_signer_who_may_update_the_parent() {
    peer_chunk_requires_parent_update_permission(false);
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.protected")]
fn an_encrypted_peer_chunk_merges_only_from_a_signer_who_may_update_the_parent() {
    peer_chunk_requires_parent_update_permission(true);
}

fn peer_chunk_requires_parent_update_permission(encrypted: bool) {
    let alice = body(PARAGRAPHS, "plans");
    let bob = body(PARAGRAPHS, "lies");
    let mut actions = alice_writes_on_db0(encrypted);
    actions.extend([
        sync("Article", vec![Text::reference("article")], "db1"),
        alice_sees("db1", &alice),
        grant("db1", "reader", false, Some("db0")),
    ]);
    actions.extend(bob_writes_on_db1(&bob));
    actions.extend([
        sync("Article", vec![Text::reference("article")], "db0"),
        sync(
            CHUNKS,
            (0..PARAGRAPHS)
                .map(|i| Text::reference(&format!("bob{i}")))
                .collect(),
            "db0",
        ),
        alice_sees("db0", &alice),
        // Alice writes Bob's text herself: her node makes its own chunks, so
        // nothing of Bob's is stored to be shown or kept.
        update_article_on("article", &bob, Some("db0"), ALICE),
        alice_sees("db0", &bob),
    ]);
    actions.extend((0..PARAGRAPHS).map(db0_has_no_commits_of_bob));
    let mut case = without_default_node(actions);
    case.client_types = Some(CLIENTS);
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.protected")]
fn a_chunk_that_arrives_before_its_parent_merges_when_the_parent_arrives() {
    let alice = body(PARAGRAPHS, "plans");
    let mut actions = alice_writes_on_db0(false);
    actions.extend([
        capture_chunks(),
        sync(CHUNKS, alice_chunks(), "db1"),
        sync("Article", vec![Text::reference("article")], "db1"),
        alice_sees("db1", &alice),
    ]);
    let mut case = without_default_node(actions);
    case.client_types = Some(CLIENTS);
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.protected")]
fn an_encrypted_protected_chunk_merges_after_its_parent() {
    let alice = body(PARAGRAPHS, "plans");
    let mut actions = alice_writes_on_db0(true);
    actions.extend([
        capture_chunks(),
        sync("Article", vec![Text::reference("article")], "db1"),
        sync(CHUNKS, alice_chunks(), "db1"),
        alice_sees("db1", &alice),
    ]);
    let mut case = without_default_node(actions);
    case.client_types = Some(CLIENTS);
    case.execute();
}

//! A store purge (`POST /api/v0/purge`, `Node::purge_as`) wipes the whole
//! store and starts the node again with the same options, as Go's
//! `PurgeAndRestart`.

use integration_test::actions::{
    Actions, AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc, GetCollections,
    GetNodeIdentity, NewNode, PeerInfo, Purge, Request, Restart,
};
use integration_test::m;
use integration_test::nodes::ProcessBinary;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

const POLICY: &str = r#"
name: test
resources:
- name: users
  permissions:
  - name: read
    expr: reader
  - name: update
  - name: delete
  relations:
  - name: admin
    manages:
    - reader
    types:
    - actor
  - name: reader
    types:
    - actor
"#;

fn as_identity(seed: u64) -> Common {
    Common {
        identity_seed: Some(seed),
        ..Default::default()
    }
}

/// Adds the policy, the `Users` collection under it and identity 1's
/// document; `round` keeps each round's captures apart.
fn users_under_policy(round: u32) -> Vec<Actions> {
    let policy = format!("policy{round}");
    vec![
        AddDACPolicy {
            policy: POLICY.into(),
            common: Common {
                identity_seed: Some(1),
                capture: captures([(policy.as_str(), "/policyID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: Text::concat([
                Text::from(r#"type Users @policy(id: ""#),
                Text::reference(&policy),
                Text::from(r#"", resource: "users") { name: String }"#),
            ]),
            common: as_identity(1),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "John" })),
            common: Common {
                identity_seed: Some(1),
                capture: captures([(format!("doc{round}").as_str(), "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]
}

/// Identity 2 reads `Users` and sees `expected`.
fn identity_two_reads(expected: serde_json::Value) -> Actions {
    Request {
        request: "query { Users { name } }".into(),
        expected_data: json!({ "Users": expected }),
        common: as_identity(2),
        ..Default::default()
    }
    .into()
}

fn no_collections() -> Actions {
    GetCollections {
        expected_data: json!({ "collections": [] }),
        ..Default::default()
    }
    .into()
}

fn users_collection() -> Actions {
    AddCollection {
        sdl: "type Users { name: String }".into(),
        ..Default::default()
    }
    .into()
}

/// A store with a policy, a collection, a document and a relationship;
/// purged; then the same policy, collection and document again, which
/// identity 2 can no longer read because its relationship went with the
/// old store. Making identity 2 a reader again is new, not a repeat.
fn wipe_scenario(node: NewNode) -> Vec<Actions> {
    let mut actions: Vec<Actions> = vec![node.into()];
    actions.extend(users_under_policy(0));
    actions.push(identity_two_made_reader("doc0"));
    actions.push(identity_two_reads(json!([{ "name": "John" }])));
    actions.push(Purge::default().into());
    actions.push(no_collections());
    actions.extend(users_under_policy(1));
    actions.push(identity_two_reads(json!([])));
    actions.push(identity_two_made_reader("doc1"));
    actions.push(identity_two_reads(json!([{ "name": "John" }])));
    actions
}

/// Identity 1 makes identity 2 a reader of the document captured as `doc`,
/// a relationship that did not exist yet.
fn identity_two_made_reader(doc: &str) -> Actions {
    AddDACActorRelationship {
        target_identity_seed: Some(2),
        collection: "Users".into(),
        doc_id: Text::reference(doc),
        relation: "reader".into(),
        expected_data: json!({ "existedAlready": false }),
        common: as_identity(1),
        ..Default::default()
    }
    .into()
}

fn run(id: &'static str, actions: Vec<Actions>) {
    TestCase {
        id,
        actions: actions.into_iter().map(Into::into).collect(),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.store_purge.wipes")]
fn purge_removes_every_collection_and_document() {
    run(
        integration_test::scenario_id!(),
        wipe_scenario(NewNode {
            // Go's process node purges only with `--development`.
            development: Some(true),
            database_dir: Some(String::new()),
            ..Default::default()
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.store_purge.persists")]
fn a_purged_persistent_node_stays_empty_after_a_restart() {
    run(
        integration_test::scenario_id!(),
        vec![
            NewNode {
                development: Some(true),
                database_dir: Some(String::new()),
                ..Default::default()
            }
            .into(),
            users_collection(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Purge::default().into(),
            Restart::default().into(),
            no_collections(),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.store_purge.restart")]
fn purge_restarts_defradb_start_on_the_same_address() {
    // The purge fails unless the node serves again at its first URL.
    run(
        integration_test::scenario_id!(),
        vec![
            NewNode {
                process: Some(ProcessBinary::Rust),
                development: Some(true),
                database_dir: Some(String::new()),
                ..Default::default()
            }
            .into(),
            users_collection(),
            Purge::default().into(),
            no_collections(),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.store_purge.keyring_kept")]
fn purge_keeps_a_keyring_held_identity_and_peer_key() {
    let identity = |name: &str| -> Actions {
        GetNodeIdentity {
            expected_data: m::same(name),
            ..Default::default()
        }
        .into()
    };
    let peer = |name: &str| -> Actions {
        PeerInfo {
            id_only: true,
            expected_data: json!({ "peerID": m::same(name) }),
            ..Default::default()
        }
        .into()
    };
    run(
        integration_test::scenario_id!(),
        vec![
            NewNode {
                process: Some(ProcessBinary::Rust),
                development: Some(true),
                keyring: true,
                network: true,
                database_dir: Some(String::new()),
                ..Default::default()
            }
            .into(),
            identity("identity"),
            peer("peer"),
            Purge::default().into(),
            identity("identity"),
            peer("peer"),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.store_purge.store_peer_key_regenerated")]
fn purge_regenerates_a_store_held_peer_key() {
    let peer = || -> Actions {
        PeerInfo {
            id_only: true,
            expected_data: json!({ "peerID": m::unique("peer") }),
            ..Default::default()
        }
        .into()
    };
    run(
        integration_test::scenario_id!(),
        vec![
            NewNode {
                process: Some(ProcessBinary::Rust),
                development: Some(true),
                network: true,
                database_dir: Some(String::new()),
                ..Default::default()
            }
            .into(),
            peer(),
            Purge::default().into(),
            peer(),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.store_purge.no_handler")]
fn purge_without_a_handler_is_refused() {
    run(
        integration_test::scenario_id!(),
        vec![
        NewNode {
            no_purge_handler: true,
            ..Default::default()
        }
        .into(),
        users_collection(),
        Purge {
            expected_error: Some(
                "purge needs the embedding application to wipe the store and restart the node: \
                 register a purge handler (NodeBuilder::with_purge_handler)"
                    .into(),
            ),
            ..Default::default()
        }
        .into(),
        GetCollections {
            expected_data: json!({ "collections": m::length(1) }),
            ..Default::default()
        }
        .into(),
    ],
    );
}

/// A node built by `node` refuses the purge for want of development mode and
/// keeps its collection.
fn refused_without_development_mode(node: NewNode) -> Vec<Actions> {
    vec![
        node.into(),
        users_collection(),
        Purge {
            expected_error: Some(
                "operation not permitted whilst development mode is disabled. Operation: Purge"
                    .into(),
            ),
            ..Default::default()
        }
        .into(),
        GetCollections {
            expected_data: json!({ "collections": m::length(1) }),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("defra.store_purge.dev_mode")]
fn purge_is_refused_without_development_mode() {
    // An embedded node with a purge handler: `Node::purge_as` in process,
    // the route over HTTP and through the CLI.
    run(
        integration_test::scenario_id!(),
        refused_without_development_mode(NewNode {
            development: Some(false),
            ..Default::default()
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.store_purge.dev_mode")]
fn defradb_start_refuses_the_purge_without_development_mode() {
    run(
        integration_test::scenario_id!(),
        refused_without_development_mode(NewNode {
            process: Some(ProcessBinary::Rust),
            ..Default::default()
        }),
    );
}

/// Whether the `defradb` under test runs an embedded Vera node.
fn embedded_vera_built() -> bool {
    std::process::Command::new(integration_test::rust_binary())
        .arg("--help")
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("--vera-embedded-operator"))
}

#[test]
#[mantra_macros::req_test("defra.store_purge.embedded_vera")]
fn purge_removes_the_embedded_vera_state() {
    if !embedded_vera_built() {
        eprintln!("skipped: defradb is built without the embedded-vera feature");
        return;
    }
    // The store is in memory, so only the Vera node's own state could keep
    // identity 2's relationship across the purge.
    run(
        integration_test::scenario_id!(),
        wipe_scenario(NewNode {
            process: Some(ProcessBinary::Rust),
            development: Some(true),
            keyring: true,
            // An embedded Vera node needs a node identity and a deployment.
            // It submits ACP changes only as an identity it holds the key
            // of, so the node identity is identity 1, the documents' owner.
            start_args: vec![
                "--document-acp-type".into(),
                "verars-embedded".into(),
                "--identity".into(),
                integration_test::scenario::identity::private_key_hex(1),
                "--vera-deployment-id".into(),
                "9407".into(),
            ],
            ..Default::default()
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.store_purge.keyring_kept")]
fn defradb_start_refuses_a_keyring_climbing_into_the_vera_state() {
    if !embedded_vera_built() {
        eprintln!("skipped: defradb is built without the embedded-vera feature");
        return;
    }
    // `x/../vera/keys` resolves inside `<rootdir>/vera`, which a purge
    // removes, without starting with it.
    run(
        integration_test::scenario_id!(),
        vec![NewNode {
            process: Some(ProcessBinary::Rust),
            development: Some(true),
            keyring: true,
            start_args: vec![
                "--document-acp-type".into(),
                "verars-embedded".into(),
                "--identity".into(),
                integration_test::scenario::identity::private_key_hex(1),
                "--vera-deployment-id".into(),
                "9407".into(),
                "--keyring-path".into(),
                "x/../vera/keys".into(),
            ],
            expected_error: Some("refusing to wipe".into()),
            ..Default::default()
        }
        .into()],
    );
}

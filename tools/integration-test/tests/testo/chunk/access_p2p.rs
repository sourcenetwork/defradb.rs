//! A peer gets a chunk's blocks only when its node can read the parent (`defra.chunk.access.p2p`).

use integration_test::actions::{
    Actions, AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc, ConnectPeers, NewNode,
    Request, SyncDocs, VerifyBlockSignature, Wait, WaitForPeersEvents,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::{ClientType, USER_ACP_POLICY};
use serde_json::json;

use super::support::{body, chunk_texts_are, eventually, on, CHUNKS};

const OWNER: u64 = 2;
const STRANGER: u64 = 3;

fn with_identity(mut common: Common, seed: u64) -> Common {
    common.identity_seed = Some(seed);
    common
}

fn node_with_articles(node: &str) -> Vec<Actions> {
    let policy = format!("policy_{node}");
    vec![
        NewNode {
            network: true,
            with_identity: true,
            common: on(Some(node)),
            ..Default::default()
        }
        .into(),
        AddDACPolicy {
            policy: USER_ACP_POLICY.into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
            common: Common {
                capture: captures([(policy.as_str(), "/policyID")]),
                ..with_identity(on(Some(node)), OWNER)
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: Text::concat([
                Text::from(r#"type Article @policy(id: ""#),
                Text::reference(&policy),
                Text::from(
                    r#"", resource: "users") {
                        title: String
                        body: String @chunk(maxChars: 64)
                    }"#,
                ),
            ]),
            common: on(Some(node)),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on(Some(node)),
            ..Default::default()
        }
        .into(),
    ]
}

fn add_article(text: &str, owner: u64, capture: &str) -> Actions {
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "title": capture, "body": text })),
        common: Common {
            retry: true,
            capture: captures([(capture, "/docID")]),
            ..with_identity(on(Some("db0")), owner)
        },
        ..Default::default()
    }
    .into()
}

/// Names the `text` block of each of the stranger's three chunks on db0 `hidden0` to `hidden2`.
fn name_hidden_blocks() -> Actions {
    Request {
        request: r#"query { _commits(filter: {fieldName: {_eq: "text"}}) { cid } }"#.into(),
        expected_data: json!({ "_commits": m::length(3) }),
        common: Common {
            capture: captures([
                ("hidden0", "/data/_commits/0/cid"),
                ("hidden1", "/data/_commits/1/cid"),
                ("hidden2", "/data/_commits/2/cid"),
            ]),
            ..with_identity(on(Some("db0")), STRANGER)
        },
        ..Default::default()
    }
    .into()
}

fn sync(collection: &str, doc_id: &str) -> Actions {
    SyncDocs {
        collection: collection.into(),
        doc_ids: vec![Text::reference(doc_id)],
        concurrency: 1,
        common: on(Some("db1")),
        ..Default::default()
    }
    .into()
}

/// db1 does not hold the block: verifying it finds nothing to load.
fn absent_on_db1(cid: &str) -> Actions {
    VerifyBlockSignature {
        cid: Text::reference(cid),
        signer_seed: Some(STRANGER),
        expected_error: Some("ipld: could not find".into()),
        common: on(Some("db1")),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.access.p2p")]
fn a_peer_gets_chunk_blocks_only_for_parents_its_node_can_read() {
    let shared = body(2, "rivers");
    let hidden = body(3, "secrets");
    let mut actions = node_with_articles("db0");
    actions.extend(node_with_articles("db1"));
    actions.extend([
        ConnectPeers {
            target: "db0".into(),
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
        add_article(&hidden, STRANGER, "hidden"),
        name_hidden_blocks(),
        Request {
            request: format!("query {{ {CHUNKS}(order: {{hash: ASC}}) {{ _docID }} }}").into(),
            common: Common {
                capture: captures([("hiddenChunk", &format!("/data/{CHUNKS}/0/_docID"))]),
                ..with_identity(on(Some("db0")), STRANGER)
            },
            ..Default::default()
        }
        .into(),
        add_article(&shared, OWNER, "shared"),
        AddDACActorRelationship {
            collection: "Article".into(),
            doc_id: Text::reference("shared"),
            relation: "reader".into(),
            target_identity_node: Some("db1".into()),
            expected_data: json!({ "existedAlready": false }),
            common: with_identity(on(Some("db0")), OWNER),
            ..Default::default()
        }
        .into(),
        sync("Article", "shared"),
        eventually(
            Request {
                request: "query { Article { title } }".into(),
                expected_data: json!({ "Article": [{ "title": "shared" }] }),
                common: with_identity(on(Some("db1")), OWNER),
                ..Default::default()
            }
            .into(),
        ),
        {
            let mut read = chunk_texts_are(&[&shared], Some("db1"));
            if let Actions::Request(request) = &mut read {
                request.common.identity_seed = Some(OWNER);
            }
            eventually(read)
        },
        sync("Article", "hidden"),
        sync(CHUNKS, "hiddenChunk"),
        // A block that never arrives raises no event to wait on, so the
        // fetches get many times what an allowed fetch takes.
        Wait {
            millis: Some(1000),
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
        absent_on_db1("hidden0"),
        absent_on_db1("hidden1"),
        absent_on_db1("hidden2"),
    ]);
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    // Document sync is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case.execute();
}

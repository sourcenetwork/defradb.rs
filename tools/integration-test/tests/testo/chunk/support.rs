//! Shared helpers for the chunk tests.

use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDoc, AddReplicator, NewNode, Request,
    WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

pub const SCHEMA: &str = r#"type Article {
    title: String
    body: String @chunk(maxChars: 64)
}"#;

pub const CHUNKS: &str = "Article__body_chunk";

/// A body of `n` short paragraphs about `topic`.
pub fn body(n: usize, topic: &str) -> String {
    (0..n)
        .map(|i| format!("Paragraph {i} is about {topic} and nothing else."))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `body` escaped for a GraphQL string.
pub fn gql(body: &str) -> String {
    body.replace('\n', "\\n")
}

pub fn add_article_collection() -> Actions {
    AddCollection {
        sdl: SCHEMA.into(),
        ..Default::default()
    }
    .into()
}

pub fn on(node: Option<&str>) -> Common {
    Common {
        node: node.map(str::to_owned),
        ..Default::default()
    }
}

/// Adds an article with `body`, naming its docID `capture`.
pub fn add_article(body: &str, capture: &str) -> Actions {
    add_article_on(body, capture, None)
}

pub fn add_article_on(body: &str, capture: &str, node: Option<&str>) -> Actions {
    let mut common = on(node);
    common.retry = true;
    common.capture = captures([(capture, "/docID")]);
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "title": "t", "body": body })),
        common,
        ..Default::default()
    }
    .into()
}

/// Updates the body of the article named `capture` through GraphQL.
pub fn update_article(capture: &str, body: &str) -> Actions {
    Request {
        request: Text::concat([
            Text::from(r#"mutation { update_Article(docID: ""#),
            Text::reference(capture),
            Text::from(format!(
                r#"", input: {{body: "{}"}}) {{ _docID }} }}"#,
                gql(body)
            )),
        ]),
        ..Default::default()
    }
    .into()
}

/// Reads the parent's chunk list, which holds the text's hash and then `n`
/// chunk hashes, and names them `<prefix>0` to `<prefix><n>`.
pub fn read_list(prefix: &str, n: usize, node: Option<&str>) -> Actions {
    let names: Vec<(String, String)> = (0..=n)
        .map(|i| {
            (
                format!("{prefix}{i}"),
                format!("/data/Article/0/_body_chunks/{i}"),
            )
        })
        .collect();
    let mut common = on(node);
    common.capture = names.into_iter().collect();
    Request {
        request: "query { Article { _body_chunks } }".into(),
        expected_data: json!({ "Article": [{ "_body_chunks": integration_test::m::length(n as u64 + 1) }] }),
        common,
        ..Default::default()
    }
    .into()
}

/// The visible chunks are exactly the `n` hashes `read_list` named.
pub fn chunks_are_listed(prefix: &str, n: usize, node: Option<&str>) -> Actions {
    let hashes: Vec<Value> = (1..=n)
        .map(|i| json!({ "hash": integration_test::m::r(&format!("{prefix}{i}")) }))
        .collect();
    let mut common = on(node);
    common.unordered = vec![format!("/data/{CHUNKS}")];
    Request {
        request: format!("query {{ {CHUNKS} {{ hash }} }}").into(),
        expected_data: json!({ CHUNKS: hashes }),
        common,
        ..Default::default()
    }
    .into()
}

pub fn no_chunks(node: Option<&str>) -> Actions {
    Request {
        request: format!("query {{ {CHUNKS} {{ hash }} }}").into(),
        expected_data: json!({ CHUNKS: [] }),
        common: on(node),
        ..Default::default()
    }
    .into()
}

/// The parent's chunk list is the one `read_list` named.
pub fn list_is(prefix: &str, n: usize, node: Option<&str>) -> Actions {
    let list: Vec<Value> = (0..=n)
        .map(|i| integration_test::m::r(&format!("{prefix}{i}")))
        .collect();
    Request {
        request: "query { Article { _body_chunks } }".into(),
        expected_data: json!({ "Article": [{ "_body_chunks": list }] }),
        common: on(node),
        ..Default::default()
    }
    .into()
}

/// Polls a read until the peer has caught up.
pub fn eventually(action: Actions) -> Actions {
    match action {
        Actions::Request(mut request) => {
            request.common.eventually = Some(Eventually::default());
            request.into()
        }
        other => other,
    }
}

/// Two networked nodes, `db0` pushing `collections` to `db1`, both with the Article collection.
pub fn replicated_pair(collections: &[&str]) -> Vec<Actions> {
    let names: Vec<String> = collections.iter().map(|name| (*name).to_owned()).collect();
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
    }
    for node in ["db0", "db1"] {
        actions.push(
            AddCollection {
                sdl: SCHEMA.into(),
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in ["db0", "db1"] {
        actions.push(
            WaitForPeersEvents {
                start: true,
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in ["db0", "db1"] {
        actions.push(
            AddCollectionSubscription {
                collections: names.clone(),
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        AddReplicator {
            target: "db1".into(),
            collections: names,
            common: on(Some("db0")),
            ..Default::default()
        }
        .into(),
    );
    actions
}

/// A case whose nodes are all named by its actions.
pub fn without_default_node(actions: Vec<Actions>) -> TestCase {
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    case
}

/// Runs a read as client identity `seed`.
pub fn as_identity(action: Actions, seed: u64) -> Actions {
    match action {
        Actions::Request(mut request) => {
            request.common.identity_seed = Some(seed);
            request.into()
        }
        other => other,
    }
}

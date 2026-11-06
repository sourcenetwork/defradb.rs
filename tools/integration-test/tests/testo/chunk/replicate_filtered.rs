//! A filtered replicator ships the chunks of the parents it matches (`defra.chunk.replicate.filtered`).

use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddReplicator, DeleteDoc, NewNode, Request, TxnBegin,
    TxnCommit, UpdateDoc, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Text};
use serde_json::{json, Value};

use super::support::{
    body, chunks_are_listed, eventually, gql, list_is, no_chunks, on, read_list,
    without_default_node,
};

const SCHEMA: &str = r#"type Article {
    kind: String @immutable
    body: String @chunk(maxChars: 64)
}"#;

fn two_nodes() -> Vec<Actions> {
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
    actions
}

/// `db0` pushes to `db1` only the articles whose kind is public. No node
/// subscribes to the collection, so nothing else reaches `db1`.
fn public_only_replicator() -> Actions {
    AddReplicator {
        target: "db1".into(),
        collections: vec!["Article".into()],
        filter: Some(json!({ "kind": { "_eq": "public" } })),
        common: on(Some("db0")),
        ..Default::default()
    }
    .into()
}

fn add_article(kind: &str, text: &str) -> Actions {
    let mut common = on(Some("db0"));
    common.retry = true;
    common.capture = captures([(kind, "/docID")]);
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "kind": kind, "body": text })),
        common,
        ..Default::default()
    }
    .into()
}

fn articles_are(kinds: &[&str], node: &str) -> Actions {
    let articles: Vec<Value> = kinds.iter().map(|kind| json!({ "kind": kind })).collect();
    Request {
        request: "query { Article { kind } }".into(),
        expected_data: json!({ "Article": articles }),
        common: on(Some(node)),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_filtered_replicator_ships_the_chunks_of_matching_parents() {
    let mut actions = two_nodes();
    actions.extend([
        public_only_replicator(),
        add_article("secret", &body(3, "caves")),
        add_article("public", &body(4, "meadows")),
        read_public_list("created", 4),
        eventually(list_is("created", 4, Some("db1"))),
        eventually(chunks_are_listed("created", 4, Some("db1"))),
        articles_are(&["public"], "db1"),
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("public"),
            doc_map: json!({ "body": body(2, "hills") }),
            common: Common {
                node: Some("db0".into()),
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        read_public_list("updated", 2),
        eventually(list_is("updated", 2, Some("db1"))),
        eventually(chunks_are_listed("updated", 2, Some("db1"))),
    ]);
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_filtered_replicator_replays_the_chunks_of_existing_matching_parents() {
    let mut actions = two_nodes();
    actions.extend([
        add_article("secret", &body(3, "caves")),
        add_article("public", &body(4, "meadows")),
        read_public_list("created", 4),
        public_only_replicator(),
        eventually(list_is("created", 4, Some("db1"))),
        eventually(chunks_are_listed("created", 4, Some("db1"))),
        articles_are(&["public"], "db1"),
    ]);
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_matching_parent_delete_removes_its_chunks_on_the_filtered_peer() {
    let mut actions = two_nodes();
    actions.extend([
        public_only_replicator(),
        add_article("public", &body(4, "meadows")),
        read_public_list("created", 4),
        eventually(chunks_are_listed("created", 4, Some("db1"))),
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("public"),
            common: Common {
                node: Some("db0".into()),
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        eventually(articles_are(&[], "db1")),
        no_chunks(Some("db1")),
    ]);
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn an_explicit_transaction_ships_the_chunks_of_a_matching_parent() {
    let add_in_txn = |kind: &str, text: &str| -> Actions {
        Request {
            request: format!(
                r#"mutation {{ add_Article(input: {{kind: "{kind}", body: "{}"}}) {{ _docID }} }}"#,
                gql(text)
            )
            .into(),
            txn: Some(0),
            common: on(Some("db0")),
            ..Default::default()
        }
        .into()
    };
    let mut actions = two_nodes();
    actions.extend([
        public_only_replicator(),
        TxnBegin {
            common: on(Some("db0")),
            ..Default::default()
        }
        .into(),
        add_in_txn("secret", &body(3, "caves")),
        add_in_txn("public", &body(4, "meadows")),
        TxnCommit {
            txn: 0,
            common: on(Some("db0")),
            ..Default::default()
        }
        .into(),
        read_public_list("committed", 4),
        eventually(list_is("committed", 4, Some("db1"))),
        eventually(chunks_are_listed("committed", 4, Some("db1"))),
        articles_are(&["public"], "db1"),
    ]);
    without_default_node(actions).execute();
}

/// `read_list` for the public article on `db0`, which also holds a secret one.
fn read_public_list(prefix: &str, n: usize) -> Actions {
    let mut list = read_list(prefix, n, Some("db0"));
    if let Actions::Request(request) = &mut list {
        request.request = r#"query { Article(filter: {kind: {_eq: "public"}}) { _body_chunks } }"#.into();
    }
    list
}

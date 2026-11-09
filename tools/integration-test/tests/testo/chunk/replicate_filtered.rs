//! A filtered replicator ships the chunks of the parents it matches (`defra.chunk.replicate.filtered`).

use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddReplicator, DeleteDoc, NewNode, Request, TxnBegin,
    TxnCommit, UpdateDoc, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Text};
use serde_json::{json, Value};

use super::support::{
    body, chunks_are_listed, eventually, gql, no_chunks, on, without_default_node,
};

const SCHEMA: &str = r#"type Article {
    kind: String @immutable
    body: String @chunk(maxChars: 64)
}"#;

const PUBLIC: &str = "public";
const SECRET: &str = "secret";
/// The name of a public article written last, so that once it arrives every
/// earlier push has been decided.
const LATER: &str = "later";
/// The name the chunk list of an edited article is read under.
const EDITED: &str = "edited";
/// The public article's paragraphs, one chunk each.
const PUBLIC_PARAGRAPHS: usize = 4;
const SECRET_PARAGRAPHS: usize = 3;
/// The paragraphs an edit leaves in an article.
const EDITED_PARAGRAPHS: usize = 2;
const LATER_PARAGRAPHS: usize = 1;
/// The first explicit transaction a scenario opens, and the second.
const TXN: usize = 0;
const SECOND_TXN: usize = 1;

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
        filter: json!({ "kind": { "_eq": PUBLIC } }),
        common: on(Some("db0")),
        ..Default::default()
    }
    .into()
}

/// Adds an article of `kind` on `db0`, naming its docID `name`.
fn add_article(kind: &str, name: &str, paragraphs: usize) -> Actions {
    let mut common = on(Some("db0"));
    common.retry = true;
    common.capture = captures([(name, "/docID")]);
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "kind": kind, "body": body(paragraphs, name) })),
        common,
        ..Default::default()
    }
    .into()
}

fn on_db0(txn: Option<usize>) -> Common {
    Common {
        node: Some("db0".into()),
        retry: txn.is_none(),
        ..Default::default()
    }
}

fn edit(name: &str, txn: Option<usize>) -> Actions {
    UpdateDoc {
        collection: "Article".into(),
        doc_id: Text::reference(name),
        doc_map: json!({ "body": body(EDITED_PARAGRAPHS, EDITED) }),
        txn,
        common: on_db0(txn),
        ..Default::default()
    }
    .into()
}

fn delete(name: &str, txn: Option<usize>) -> Actions {
    DeleteDoc {
        collection: "Article".into(),
        doc_id: Text::reference(name),
        txn,
        common: on_db0(txn),
        ..Default::default()
    }
    .into()
}

fn begin() -> Actions {
    TxnBegin {
        common: on(Some("db0")),
        ..Default::default()
    }
    .into()
}

fn commit(txn: usize) -> Actions {
    TxnCommit {
        txn,
        common: on(Some("db0")),
        ..Default::default()
    }
    .into()
}

fn list_query(name: &str) -> Text {
    Text::concat([
        r#"query { Article(docID: ""#.into(),
        Text::reference(name),
        r#"") { _body_chunks } }"#.into(),
    ])
}

/// Reads the chunk list of `db0`'s article `name`, the text's hash and then
/// `n` chunk hashes, and names them `<prefix>0` to `<prefix><n>`.
fn read_list(name: &str, prefix: &str, n: usize) -> Actions {
    let mut common = on(Some("db0"));
    common.capture = (0..=n)
        .map(|i| {
            (
                format!("{prefix}{i}"),
                format!("/data/Article/0/_body_chunks/{i}"),
            )
        })
        .collect();
    Request {
        request: list_query(name),
        expected_data: json!({ "Article": [{ "_body_chunks": integration_test::m::length(n as u64 + 1) }] }),
        common,
        ..Default::default()
    }
    .into()
}

/// `db1`'s article `name` eventually has the list read under `prefix`.
fn peer_list_is(name: &str, prefix: &str, n: usize) -> Actions {
    let list: Vec<Value> = (0..=n)
        .map(|i| integration_test::m::r(&format!("{prefix}{i}")))
        .collect();
    eventually(
        Request {
            request: list_query(name),
            expected_data: json!({ "Article": [{ "_body_chunks": list }] }),
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
    )
}

/// `db1` shows exactly the articles of `kinds`.
fn peer_articles_are(kinds: &[&str]) -> Actions {
    let articles: Vec<Value> = kinds.iter().map(|kind| json!({ "kind": kind })).collect();
    let mut common = on(Some("db1"));
    common.unordered = vec!["/data/Article".into()];
    Request {
        request: "query { Article { kind } }".into(),
        expected_data: json!({ "Article": articles }),
        common,
        ..Default::default()
    }
    .into()
}

/// `db1` shows article `name` with the list read under `prefix`, and exactly
/// that list's chunks.
fn peer_has_chunks(name: &str, prefix: &str, n: usize) -> [Actions; 2] {
    [
        peer_list_is(name, prefix, n),
        eventually(chunks_are_listed(prefix, n, Some("db1"))),
    ]
}

/// A later public article reaches `db1`, so every earlier push has been decided.
fn after_a_later_public_write() -> [Actions; 3] {
    [
        add_article(PUBLIC, LATER, LATER_PARAGRAPHS),
        read_list(LATER, LATER, LATER_PARAGRAPHS),
        peer_list_is(LATER, LATER, LATER_PARAGRAPHS),
    ]
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_filtered_replicator_ships_the_chunks_of_a_matching_parent() {
    let mut actions = two_nodes();
    actions.push(public_only_replicator());
    actions.push(add_article(SECRET, SECRET, SECRET_PARAGRAPHS));
    actions.push(add_article(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.push(read_list(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.extend(peer_has_chunks(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.extend(after_a_later_public_write());
    actions.push(peer_articles_are(&[PUBLIC, PUBLIC]));
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_filtered_replicator_ships_the_new_chunks_of_an_edited_matching_parent() {
    let mut actions = two_nodes();
    actions.push(public_only_replicator());
    actions.push(add_article(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.push(read_list(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.extend(peer_has_chunks(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.push(edit(PUBLIC, None));
    actions.push(read_list(PUBLIC, EDITED, EDITED_PARAGRAPHS));
    actions.extend(peer_has_chunks(PUBLIC, EDITED, EDITED_PARAGRAPHS));
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_filtered_replicator_replays_the_chunks_of_existing_matching_parents() {
    let mut actions = two_nodes();
    actions.push(add_article(SECRET, SECRET, SECRET_PARAGRAPHS));
    actions.push(add_article(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.push(read_list(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.push(public_only_replicator());
    actions.extend(peer_has_chunks(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.extend(after_a_later_public_write());
    actions.push(peer_articles_are(&[PUBLIC, PUBLIC]));
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_matching_parent_delete_removes_its_chunks_on_the_filtered_peer() {
    let mut actions = two_nodes();
    actions.push(public_only_replicator());
    actions.push(add_article(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.push(read_list(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.extend(peer_has_chunks(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.push(delete(PUBLIC, None));
    actions.push(eventually(peer_articles_are(&[])));
    actions.push(no_chunks(Some("db1")));
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn an_unmatched_parent_edit_and_delete_send_nothing() {
    let mut actions = two_nodes();
    actions.push(public_only_replicator());
    actions.push(add_article(SECRET, SECRET, SECRET_PARAGRAPHS));
    actions.push(edit(SECRET, None));
    actions.push(delete(SECRET, None));
    actions.extend(after_a_later_public_write());
    actions.push(peer_articles_are(&[PUBLIC]));
    actions.push(chunks_are_listed(LATER, LATER_PARAGRAPHS, Some("db1")));
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn an_explicit_transaction_ships_the_chunks_of_a_matching_parent() {
    let add_in_txn = |kind: &str, paragraphs: usize| -> Actions {
        let mut common = on(Some("db0"));
        common.capture = captures([(kind, "/data/add_Article/0/_docID")]);
        Request {
            request: format!(
                r#"mutation {{ add_Article(input: {{kind: "{kind}", body: "{}"}}) {{ _docID }} }}"#,
                gql(&body(paragraphs, kind))
            )
            .into(),
            txn: Some(TXN),
            common,
            ..Default::default()
        }
        .into()
    };
    let mut actions = two_nodes();
    actions.push(public_only_replicator());
    actions.push(begin());
    actions.push(add_in_txn(SECRET, SECRET_PARAGRAPHS));
    actions.push(add_in_txn(PUBLIC, PUBLIC_PARAGRAPHS));
    actions.push(commit(TXN));
    actions.push(read_list(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.extend(peer_has_chunks(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.extend(after_a_later_public_write());
    actions.push(peer_articles_are(&[PUBLIC, PUBLIC]));
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn an_explicit_transaction_edit_and_delete_reach_the_filtered_peer() {
    let mut actions = two_nodes();
    actions.push(public_only_replicator());
    actions.push(add_article(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.push(read_list(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.extend(peer_has_chunks(PUBLIC, PUBLIC, PUBLIC_PARAGRAPHS));
    actions.push(begin());
    actions.push(edit(PUBLIC, Some(TXN)));
    actions.push(commit(TXN));
    actions.push(read_list(PUBLIC, EDITED, EDITED_PARAGRAPHS));
    actions.extend(peer_has_chunks(PUBLIC, EDITED, EDITED_PARAGRAPHS));
    actions.push(begin());
    actions.push(delete(PUBLIC, Some(SECOND_TXN)));
    actions.push(commit(SECOND_TXN));
    actions.push(eventually(peer_articles_are(&[])));
    actions.push(no_chunks(Some("db1")));
    without_default_node(actions).execute();
}

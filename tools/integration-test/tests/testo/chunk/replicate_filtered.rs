//! A filtered replicator ships a matching parent with its chunks (`defra.chunk.replicate.filtered`).

use integration_test::actions::{
    Actions, AddCollection, AddDoc, AddReplicator, Close, ConnectPeers, DeleteDoc, NewNode,
    Request, Start, TxnBegin, TxnCommit, UpdateDoc, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

use super::support::{body, chunk_texts_are, eventually, gql, on, CHUNKS};

mod relay;
mod remove;

// A replicator filter matches immutable fields only.
const SCHEMA: &str = r#"type Article {
    kind: String @immutable
    body: String @chunk(maxChars: 64)
}"#;

/// The first explicit transaction a scenario opens, and the second.
const TXN: usize = 0;
const SECOND_TXN: usize = 1;

fn public_body() -> String {
    body(3, "rivers")
}

fn secret_body() -> String {
    body(2, "lakes")
}

fn edited_body() -> String {
    body(2, "edited rivers")
}

/// The body of a public article written last, so that once its chunks
/// arrive every earlier push has been decided.
fn later_body() -> String {
    body(1, "later")
}

fn node(name: &str, persistent: bool) -> Actions {
    NewNode {
        network: true,
        database_dir: persistent.then(String::new),
        common: on(Some(name)),
        ..Default::default()
    }
    .into()
}

fn two_nodes(persistent: bool) -> Vec<Actions> {
    [("db0", persistent), ("db1", persistent)]
        .into_iter()
        .flat_map(|(name, persistent)| node_with(name, persistent, SCHEMA))
        .collect()
}

/// A started node named `name` that declares `sdl`.
fn node_with(name: &str, persistent: bool, sdl: &str) -> Vec<Actions> {
    vec![
        node(name, persistent),
        AddCollection {
            sdl: sdl.into(),
            common: on(Some(name)),
            ..Default::default()
        }
        .into(),
        started(name),
    ]
}

fn started(name: &str) -> Actions {
    WaitForPeersEvents {
        start: true,
        common: on(Some(name)),
        ..Default::default()
    }
    .into()
}

/// Stops `name` and starts it again on the same data.
fn restart(name: &str) -> Vec<Actions> {
    vec![
        Close {
            keep_data: true,
            common: on(Some(name)),
            ..Default::default()
        }
        .into(),
        Start {
            enable_nac: false,
            common: on(Some(name)),
            ..Default::default()
        }
        .into(),
        started(name),
    ]
}

fn connect(from: &str, to: &str) -> Actions {
    ConnectPeers {
        target: to.into(),
        common: on(Some(from)),
        ..Default::default()
    }
    .into()
}

fn replicator_with(filter: Value) -> Actions {
    AddReplicator {
        target: "db1".into(),
        collections: vec!["Article".into()],
        filter,
        common: on(Some("db0")),
        ..Default::default()
    }
    .into()
}

/// `db0` pushes to `db1` only the public articles. No node subscribes to the
/// collection, so nothing else reaches `db1`.
fn replicator() -> Actions {
    replicator_with(json!({ "kind": { "_eq": "public" } }))
}

/// A negative filter: a chunk has no `kind`, so on its own document every
/// chunk would pass it.
fn not_secret_replicator() -> Actions {
    replicator_with(json!({ "kind": { "_ne": "secret" } }))
}

/// Adds an article of `kind` on `db0`, naming its docID `kind`.
fn article(kind: &str, text: &str) -> Actions {
    article_on("db0", kind, kind, text)
}

/// Adds an article of `kind` on `node`, naming its docID `name`.
fn article_on(node: &str, name: &str, kind: &str, text: &str) -> Actions {
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "kind": kind, "body": text })),
        common: Common {
            retry: true,
            capture: captures([(name, "/docID")]),
            ..on(Some(node))
        },
        ..Default::default()
    }
    .into()
}

/// The public article written last on `db0`; see [`later_body`].
fn later_article() -> Actions {
    article_on("db0", "later", "public", &later_body())
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
        doc_map: json!({ "body": edited_body() }),
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

/// `db1` eventually shows exactly the articles of `kinds`.
fn peer_articles_are(kinds: &[&str]) -> Actions {
    articles_are("db1", kinds)
}

/// `node` eventually shows exactly the articles of `kinds`.
fn articles_are(node: &str, kinds: &[&str]) -> Actions {
    let articles: Vec<Value> = kinds.iter().map(|kind| json!({ "kind": kind })).collect();
    let mut common = on(Some(node));
    common.unordered = vec!["/data/Article".into()];
    eventually(
        Request {
            request: "query { Article { kind } }".into(),
            expected_data: json!({ "Article": articles }),
            common,
            ..Default::default()
        }
        .into(),
    )
}

/// `db1` eventually holds exactly the chunks of `bodies`.
fn peer_chunks_are(bodies: &[&str]) -> Actions {
    chunks_are("db1", bodies)
}

/// `node` eventually holds exactly the chunks of `bodies`.
fn chunks_are(node: &str, bodies: &[&str]) -> Actions {
    eventually(chunk_texts_are(bodies, Some(node)))
}

fn filtered(actions: Vec<Actions>) -> TestCase {
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    // Filtered replication is defradb.rs only.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]);
    case
}

fn ships_public_chunks(replicator_first: bool) {
    let (public, secret) = (public_body(), secret_body());
    let mut actions = two_nodes(false);
    if replicator_first {
        actions.push(replicator());
    }
    actions.extend([article("secret", &secret), article("public", &public)]);
    if !replicator_first {
        actions.push(replicator());
    }
    actions.extend([
        later_article(),
        peer_chunks_are(&[&public, &later_body()]),
        peer_articles_are(&["public", "public"]),
    ]);
    filtered(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_filtered_replicator_ships_the_matching_parents_chunks() {
    ships_public_chunks(true);
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_filtered_replicator_replays_the_matching_parents_chunks() {
    ships_public_chunks(false);
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_filtered_replicator_ships_the_new_chunks_of_an_edited_matching_parent() {
    let (public, edited) = (public_body(), edited_body());
    let mut actions = two_nodes(false);
    actions.extend([
        replicator(),
        article("public", &public),
        peer_chunks_are(&[&public]),
        edit("public", None),
        peer_chunks_are(&[&edited]),
    ]);
    filtered(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn a_matching_parent_delete_removes_its_chunks_on_the_filtered_peer() {
    let public = public_body();
    let mut actions = two_nodes(false);
    actions.extend([
        replicator(),
        article("public", &public),
        peer_chunks_are(&[&public]),
        delete("public", None),
        peer_articles_are(&[]),
        peer_chunks_are(&[]),
    ]);
    filtered(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn an_unmatched_parent_edit_and_delete_send_nothing() {
    let (secret, later) = (secret_body(), later_body());
    let mut actions = two_nodes(false);
    actions.extend([
        replicator(),
        article("secret", &secret),
        edit("secret", None),
        delete("secret", None),
        article("public", &later),
        peer_chunks_are(&[&later]),
        peer_articles_are(&["public"]),
    ]);
    filtered(actions).execute();
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
            txn: Some(TXN),
            common: on(Some("db0")),
            ..Default::default()
        }
        .into()
    };
    let (public, secret) = (public_body(), secret_body());
    let mut actions = two_nodes(false);
    actions.extend([
        replicator(),
        begin(),
        add_in_txn("secret", &secret),
        add_in_txn("public", &public),
        commit(TXN),
        later_article(),
        peer_chunks_are(&[&public, &later_body()]),
        peer_articles_are(&["public", "public"]),
    ]);
    filtered(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered")]
fn an_explicit_transaction_edit_and_delete_reach_the_filtered_peer() {
    let (public, edited) = (public_body(), edited_body());
    let mut actions = two_nodes(false);
    actions.extend([
        replicator(),
        article("public", &public),
        peer_chunks_are(&[&public]),
        begin(),
        edit("public", Some(TXN)),
        commit(TXN),
        peer_chunks_are(&[&edited]),
        begin(),
        delete("public", Some(SECOND_TXN)),
        commit(SECOND_TXN),
        peer_articles_are(&[]),
        peer_chunks_are(&[]),
    ]);
    filtered(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered.negative")]
fn a_negative_filter_ships_only_the_chunks_of_the_parents_it_matches() {
    let (public, secret) = (public_body(), secret_body());
    let mut actions = two_nodes(false);
    actions.extend([
        not_secret_replicator(),
        article("secret", &secret),
        article("public", &public),
        later_article(),
        peer_chunks_are(&[&public, &later_body()]),
        peer_articles_are(&["public", "public"]),
    ]);
    filtered(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered.negative")]
fn a_negative_filter_replays_only_the_chunks_of_the_parents_it_matches() {
    let (public, secret) = (public_body(), secret_body());
    let mut actions = two_nodes(false);
    actions.extend([
        article("secret", &secret),
        article("public", &public),
        not_secret_replicator(),
        later_article(),
        peer_chunks_are(&[&public, &later_body()]),
        peer_articles_are(&["public", "public"]),
    ]);
    filtered(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered.negative")]
fn a_negative_filter_retries_only_the_chunks_of_the_parents_it_matches() {
    let (public, secret) = (public_body(), secret_body());
    let mut actions = two_nodes(true);
    actions.extend([
        not_secret_replicator(),
        Close {
            keep_data: true,
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
        article("secret", &secret),
        article("public", &public),
        Start {
            enable_nac: false,
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
        started("db1"),
        connect("db0", "db1"),
        later_article(),
        peer_chunks_are(&[&public, &later_body()]),
        peer_articles_are(&["public", "public"]),
    ]);
    filtered(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered.chunk_filter")]
fn a_replicator_filter_on_a_chunk_collection_is_refused() {
    let mut actions = two_nodes(false);
    actions.push(
        AddReplicator {
            target: "db1".into(),
            collections: vec![CHUNKS.into()],
            filter: json!({ "hash": { "_ne": "" } }),
            expected_error: Some(format!(
                "a replicator filter can't name chunk collection '{CHUNKS}': filter its parent 'Article', whose filter decides its chunks"
            )),
            common: on(Some("db0")),
            ..Default::default()
        }
        .into(),
    );
    filtered(actions).execute();
}

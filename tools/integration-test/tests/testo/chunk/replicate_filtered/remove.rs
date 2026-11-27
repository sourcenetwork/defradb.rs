//! Removing a filtered parent from a replicator removes its chunks
//! (`defra.chunk.replicate.filtered.remove`).

use integration_test::actions::{
    Actions, AddDoc, AddReplicator, DeleteReplicator, GetCollections, Request,
};
use integration_test::scenario::{captures, Common, Text};
use serde_json::{json, Value};

use super::{
    article_on, body, connect, filtered, node_with, peer_articles_are, peer_chunks_are,
    public_body, restart, SCHEMA,
};
use crate::chunk::support::{eventually, on};

fn schema() -> String {
    format!("{SCHEMA}\ntype Note {{\n    kind: String @immutable\n    text: String\n}}")
}

fn replicator() -> Actions {
    AddReplicator {
        target: "db1".into(),
        collections: vec!["Article".into(), "Note".into()],
        filter: json!({ "kind": { "_eq": "public" } }),
        common: on(Some("db0")),
        ..Default::default()
    }
    .into()
}

fn note(text: &str) -> Actions {
    AddDoc {
        collection: "Note".into(),
        doc_map: Some(json!({ "kind": "public", "text": text })),
        common: Common {
            retry: true,
            ..on(Some("db0"))
        },
        ..Default::default()
    }
    .into()
}

/// `db1` eventually shows exactly the notes of `texts`.
fn peer_notes_are(texts: &[&str]) -> Actions {
    let notes: Vec<Value> = texts.iter().map(|text| json!({ "text": text })).collect();
    let mut common = on(Some("db1"));
    common.unordered = vec!["/data/Note".into()];
    eventually(
        Request {
            request: "query { Note { text } }".into(),
            expected_data: json!({ "Note": notes }),
            common,
            ..Default::default()
        }
        .into(),
    )
}

/// A public article written on `db0` sends nothing, as the public note
/// written after it shows: `db1` still has only the first article.
fn article_sends_nothing(name: &str, notes: &[&str], public: &str) -> Vec<Actions> {
    vec![
        article_on("db0", name, "public", &body(1, name)),
        note(notes.last().unwrap()),
        peer_notes_are(notes),
        peer_chunks_are(&[public]),
        peer_articles_are(&["public"]),
    ]
}

/// Removes `Article` from the replicator by `article`, its name or ID.
fn removes_the_chunks(lookup: Vec<Actions>, article: Text) {
    let public = public_body();
    let mut actions = node_with("db0", true, &schema());
    actions.extend(node_with("db1", false, &schema()));
    actions.extend([
        replicator(),
        article_on("db0", "public", "public", &public),
        peer_chunks_are(&[&public]),
    ]);
    actions.extend(lookup);
    actions.push(
        DeleteReplicator {
            target: "db1".into(),
            collections: vec![article],
            common: on(Some("db0")),
            ..Default::default()
        }
        .into(),
    );
    actions.extend(article_sends_nothing("after", &["first"], &public));
    actions.extend(restart("db0"));
    actions.push(connect("db0", "db1"));
    actions.extend(article_sends_nothing(
        "restarted",
        &["first", "second"],
        &public,
    ));
    filtered(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered.remove")]
fn removing_the_parent_by_name_removes_its_chunks() {
    removes_the_chunks(Vec::new(), "Article".into());
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered.remove")]
fn removing_the_parent_by_id_removes_its_chunks() {
    let lookup = GetCollections {
        collection_name: Some("Article".into()),
        filtered: true,
        common: Common {
            capture: captures([("article", "/collections/0/CollectionID")]),
            ..on(Some("db0"))
        },
        ..Default::default()
    };
    removes_the_chunks(vec![lookup.into()], Text::reference("article"));
}

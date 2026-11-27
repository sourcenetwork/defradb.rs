//! Encrypting peers that write the same text show each of its chunks once (`defra.chunk.replicate.encrypted`).

use integration_test::actions::{
    Actions, AddCollection, AddDoc, DisconnectPeers, NewNode, Request, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Text};
use serde_json::{json, Value};

use super::replicate_concurrent::{connect, sync};
use super::support::{
    body, eventually, on, texts, update_article_on, without_default_node, CHUNKS, SCHEMA,
};

const PARAGRAPHS: usize = 3;

fn two_kms_nodes() -> Vec<Actions> {
    let mut actions = Vec::new();
    for node in ["db0", "db1"] {
        actions.push(
            NewNode {
                network: true,
                kms: true,
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

fn add_encrypted_article(text: &str) -> Actions {
    let mut common = on(Some("db0"));
    common.capture = captures([("article", "/docID")]);
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "title": "t", "body": text })),
        encrypted_fields: Some(vec!["body".into()]),
        common,
        ..Default::default()
    }
    .into()
}

/// Takes one document short ID on `db1` that `db0` does not take.
fn pad_short_ids() -> Vec<Actions> {
    vec![
        AddCollection {
            sdl: "type Pad { n: Int }".into(),
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Pad".into(),
            doc_map: Some(json!({ "n": 1 })),
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
    ]
}

/// `db1` holds none of `db0`'s chunk documents before they sync.
fn chunk_ids_differ() -> Actions {
    let ids: Vec<Text> = (0..PARAGRAPHS)
        .map(|i| Text::reference(&format!("chunk{i}")))
        .collect();
    let mut parts = vec![Text::from(format!("query {{ {CHUNKS}(docID: ["))];
    for (i, id) in ids.into_iter().enumerate() {
        if i > 0 {
            parts.push(Text::from(", "));
        }
        parts.push(Text::from("\""));
        parts.push(id);
        parts.push(Text::from("\""));
    }
    parts.push(Text::from("]) { _docID } }"));
    Request {
        request: Text::concat(parts),
        expected_data: json!({ CHUNKS: [] }),
        common: on(Some("db1")),
        ..Default::default()
    }
    .into()
}

/// `db1` stores `db0`'s copy of the first chunk next to its own.
fn holds_db0_copy() -> Actions {
    eventually(
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("chunk0"),
                Text::from(r#"", filter: {fieldName: {_eq: "text"}}) { fieldName } }"#),
            ]),
            expected_data: json!({ "_commits": [{ "fieldName": "text" }] }),
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
    )
}

fn disconnect() -> Actions {
    DisconnectPeers {
        targets: vec!["db1".into()],
        common: on(Some("db0")),
        ..Default::default()
    }
    .into()
}

/// Names the chunk documents `node` shows `<prefix>0` to `<prefix><n-1>`.
fn read_chunk_ids(prefix: &str, node: &str) -> Actions {
    let common = Common {
        capture: (0..PARAGRAPHS)
            .map(|i| (format!("{prefix}{i}"), format!("/data/{CHUNKS}/{i}/_docID")))
            .collect(),
        ..on(Some(node))
    };
    Request {
        request: format!("query {{ {CHUNKS} {{ _docID }} }}").into(),
        expected_data: json!({ CHUNKS: integration_test::m::length(PARAGRAPHS as u64) }),
        common,
        ..Default::default()
    }
    .into()
}

/// A query that names the three documents each node built for the shared text.
fn both_chunk_ids() -> Text {
    let mut ids = Vec::with_capacity(PARAGRAPHS * 2);
    for prefix in ["chunk", "own"] {
        ids.extend((0..PARAGRAPHS).map(|i| Text::reference(&format!("{prefix}{i}"))));
    }
    let mut parts = vec![Text::from(format!("query {{ {CHUNKS}(docID: ["))];
    for (i, id) in ids.into_iter().enumerate() {
        if i > 0 {
            parts.push(Text::from(", "));
        }
        parts.push(Text::from("\""));
        parts.push(id);
        parts.push(Text::from("\""));
    }
    parts.push(Text::from("]) { _docID } }"));
    Text::concat(parts)
}

/// The six stored documents contain three visible chunks: each pair of copies
/// has one shown member, and a direct lookup of the other member returns none.
fn hidden_copies_are_not_addressable() -> Actions {
    Request {
        request: both_chunk_ids(),
        expected_data: json!({ CHUNKS: integration_test::m::length(PARAGRAPHS as u64) }),
        common: on(Some("db1")),
        ..Default::default()
    }
    .into()
}

fn chunks_are(text: &str, node: &str) -> Actions {
    let rows: Vec<Value> = texts(text)
        .into_iter()
        .map(|text| json!({ "text": text }))
        .collect();
    let mut common = on(Some(node));
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

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.encrypted")]
fn encrypting_peers_that_write_the_same_text_show_each_chunk_once() {
    let shared = body(PARAGRAPHS, "shared");
    let mut actions = two_kms_nodes();
    // Each node encrypts its chunks with a key of its own, so the two copies
    // of one chunk get different documents. Test keys are derived from the
    // node's own short IDs, so db1 draws one first to make its keys differ.
    actions.extend([
        add_encrypted_article(&body(2, "start")),
        connect(),
        sync("Article", vec![Text::reference("article")], "db1"),
        chunks_are(&body(2, "start"), "db1"),
        disconnect(),
    ]);
    actions.extend(pad_short_ids());
    actions.extend([
        update_article_on("article", &shared, Some("db0")),
        update_article_on("article", &shared, Some("db1")),
        read_chunk_ids("chunk", "db0"),
        read_chunk_ids("own", "db1"),
        chunk_ids_differ(),
        connect(),
        sync("Article", vec![Text::reference("article")], "db1"),
        sync(
            CHUNKS,
            (0..PARAGRAPHS)
                .map(|i| Text::reference(&format!("chunk{i}")))
                .collect(),
            "db1",
        ),
        holds_db0_copy(),
        hidden_copies_are_not_addressable(),
        chunks_are(&shared, "db1"),
    ]);
    without_default_node(actions).execute();
}

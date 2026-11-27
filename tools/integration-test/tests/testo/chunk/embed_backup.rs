//! A backup keeps the chunk vectors, so importing it needs no model
//! (`defra.chunk.embed.backup`). A backup without them embeds the chunks again,
//! and a failing model fails that import (`defra.chunk.embed.backup_reembeds`).

use integration_test::actions::{
    Actions, AddCollection, ExportBackup, ImportBackup, NewNode, Request,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_failure::{model_fails, MODEL_FAILED};
use super::embed_support::{
    chunks_have_vectors, chunks_of, embedded_schema, embedding_input, model_embedded,
};
use super::support::{add_article_on, body, no_chunks, on, update_article_on, CHUNKS};

const PARAGRAPHS: usize = 3;

/// Two nodes with the embedded schema, an article on `db0` and its export.
fn exported() -> Vec<Actions> {
    exported_after(None)
}

/// [`exported`], with the article's body changed to `edit` about a topic and
/// back before the export: the article keeps its create, so its `_docID`, but
/// its chunks name the commit that changed it back.
fn exported_after(edit: Option<&str>) -> Vec<Actions> {
    let chunks = chunks_of(PARAGRAPHS, "rivers");
    let inputs: Vec<String> = chunks.iter().map(|chunk| embedding_input(chunk)).collect();
    let mut actions = vec![model_embedded(&[])];
    for node in ["db0", "db1"] {
        actions.push(
            // A networked node keeps its own embedding cache, so db1 can't reuse db0's vectors.
            NewNode {
                network: true,
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddCollection {
                sdl: embedded_schema(None),
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.extend([
        add_article_on(&body(PARAGRAPHS, "rivers"), "article", Some("db0")),
        model_embedded(&inputs),
    ]);
    if let Some(topic) = edit {
        let inputs: Vec<String> = chunks_of(PARAGRAPHS, topic)
            .iter()
            .map(|chunk| embedding_input(chunk))
            .collect();
        actions.extend([
            update_article_on("article", &body(PARAGRAPHS, topic), Some("db0")),
            model_embedded(&inputs),
            // The node's embedding cache answers the texts it embedded before.
            update_article_on("article", &body(PARAGRAPHS, "rivers"), Some("db0")),
            model_embedded(&[]),
        ]);
    }
    actions.extend([ExportBackup {
        common: Common {
            node: Some("db0".into()),
            capture: captures([("backup", "/content")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()]);
    actions
}

fn import(expected_error: Option<&str>) -> Actions {
    ImportBackup {
        import_content: Text::reference("backup"),
        expected_error: expected_error.map(str::to_owned),
        common: on(Some("db1")),
        ..Default::default()
    }
    .into()
}

/// The chunk IDs on `node`, in text order, captured as `chunk0`, `chunk1`, ...
fn capture_chunk_ids(node: &str) -> Actions {
    Request {
        request: format!("query {{ {CHUNKS}(order: {{text: ASC}}) {{ _docID }} }}").into(),
        common: Common {
            node: Some(node.into()),
            capture: (0..PARAGRAPHS)
                .map(|i| (format!("chunk{i}"), format!("/data/{CHUNKS}/{i}/_docID")))
                .collect(),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// `node` holds the chunks captured by [`capture_chunk_ids`], under the same IDs.
fn same_chunk_ids(node: &str, chunks: &[String]) -> Actions {
    let mut parts = vec![Text::from(format!(
        "query {{ {CHUNKS}(order: {{text: ASC}}, docID: ["
    ))];
    for i in 0..PARAGRAPHS {
        parts.push(Text::from("\""));
        parts.push(Text::reference(&format!("chunk{i}")));
        parts.push(Text::from("\","));
    }
    parts.push(Text::from("]) { text } }"));
    let mut sorted = chunks.to_vec();
    sorted.sort();
    let expected: Vec<serde_json::Value> =
        sorted.iter().map(|text| json!({ "text": text })).collect();
    Request {
        request: Text::concat(parts),
        expected_data: json!({ CHUNKS: expected }),
        common: on(Some(node)),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.backup")]
fn importing_a_backup_keeps_the_chunk_vectors() {
    let chunks = chunks_of(PARAGRAPHS, "rivers");
    let mut actions = exported();
    actions.extend([
        capture_chunk_ids("db0"),
        model_fails().into(),
        import(None),
        model_embedded(&[]),
        chunks_have_vectors(&chunks, Some("db1")),
        same_chunk_ids("db1", &chunks),
    ]);
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.backup")]
fn importing_an_edited_articles_backup_keeps_its_chunk_ids() {
    let chunks = chunks_of(PARAGRAPHS, "rivers");
    let mut actions = exported_after(Some("lakes"));
    actions.extend([
        capture_chunk_ids("db0"),
        model_fails().into(),
        import(None),
        model_embedded(&[]),
        chunks_have_vectors(&chunks, Some("db1")),
        same_chunk_ids("db1", &chunks),
    ]);
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    case.execute();
}

/// A backup of one article with no chunk vectors, as a file from before
/// backups kept them.
fn backup_without_vectors() -> Text {
    let file = json!({ "Article": [{ "title": "t", "body": body(PARAGRAPHS, "rivers") }] });
    Text::from(file.to_string())
}

fn import_without_vectors(expected_error: Option<&str>) -> Actions {
    ImportBackup {
        import_content: backup_without_vectors(),
        expected_error: expected_error.map(str::to_owned),
        common: on(Some("db1")),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.backup_reembeds")]
fn importing_a_backup_without_vectors_embeds_the_chunks_again() {
    let chunks = chunks_of(PARAGRAPHS, "rivers");
    let inputs: Vec<String> = chunks.iter().map(|chunk| embedding_input(chunk)).collect();
    let mut actions = exported();
    actions.extend([
        import_without_vectors(None),
        model_embedded(&inputs),
        chunks_have_vectors(&chunks, Some("db1")),
    ]);
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.backup_reembeds")]
fn importing_a_backup_without_vectors_while_the_model_fails_reports_it() {
    let mut actions = exported();
    actions.extend([
        model_fails().into(),
        import_without_vectors(Some(MODEL_FAILED)),
        Request {
            request: "query { Article { _docID } }".into(),
            expected_data: json!({ "Article": [] }),
            common: on(Some("db1")),
            ..Default::default()
        }
        .into(),
        no_chunks(Some("db1")),
    ]);
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    case.execute();
}

//! Importing a backup embeds the chunks again, and a failing model fails the
//! import with the model's error (`defra.chunk.embed.backup`).

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
use super::support::{add_article_on, body, no_chunks, on};

const PARAGRAPHS: usize = 3;

/// Two nodes with the embedded schema, an article on `db0` and its export.
fn exported() -> Vec<Actions> {
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
        ExportBackup {
            common: Common {
                node: Some("db0".into()),
                capture: captures([("backup", "/content")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ]);
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

#[test]
#[mantra_macros::req_test("defra.chunk.embed.backup")]
fn importing_a_backup_embeds_the_chunks_again() {
    let chunks = chunks_of(PARAGRAPHS, "rivers");
    let inputs: Vec<String> = chunks.iter().map(|chunk| embedding_input(chunk)).collect();
    let mut actions = exported();
    actions.extend([
        import(None),
        model_embedded(&inputs),
        chunks_have_vectors(&chunks, Some("db1")),
    ]);
    let mut case = TestCase::from_actions(actions);
    case.without_db = true;
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.backup")]
fn importing_a_backup_while_the_model_fails_reports_it() {
    let mut actions = exported();
    actions.extend([
        model_fails().into(),
        import(Some(MODEL_FAILED)),
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

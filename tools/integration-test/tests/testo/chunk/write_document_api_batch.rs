//! Many-document adds, filtered updates and deletes, and saves through the document API keep every parent's chunks exact (`defra.chunk.write.document_api_batch`).

use integration_test::actions::shared::Filter;
use integration_test::actions::{AddDoc, DeleteWithFilter, UpdateWithFilter};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

use super::support::{add_article_collection, body, chunk_texts_are, no_chunks, read_chunks};

fn title_is(title: &str) -> Filter {
    Filter {
        kind: "string".into(),
        value: Some(json!(format!(r#"{{title: {{_eq: "{title}"}}}}"#))),
    }
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.document_api_batch")]
fn document_api_batch_writes_keep_each_parents_chunks_exact() {
    let (rivers, lakes, hills) = (body(3, "rivers"), body(2, "lakes"), body(2, "hills"));
    TestCase::from_actions(vec![
        add_article_collection(),
        AddDoc {
            collection: "Article".into(),
            docs: Some(vec![
                json!({ "title": "a", "body": rivers }),
                json!({ "title": "b", "body": lakes }),
            ]),
            ..Default::default()
        }
        .into(),
        chunk_texts_are(&[&rivers, &lakes], None),
        UpdateWithFilter {
            collection: "Article".into(),
            filter: title_is("a"),
            updater: json!({ "body": hills }).to_string(),
            ..Default::default()
        }
        .into(),
        chunk_texts_are(&[&hills, &lakes], None),
        DeleteWithFilter {
            collection: "Article".into(),
            filter: title_is("b"),
            ..Default::default()
        }
        .into(),
        chunk_texts_are(&[&hills], None),
        DeleteWithFilter {
            collection: "Article".into(),
            filter: title_is("a"),
            ..Default::default()
        }
        .into(),
        no_chunks(None),
    ])
    .execute();
}

fn save(doc_id: Option<&str>, text: &str) -> AddDoc {
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "title": "t", "body": text })),
        doc_id: doc_id.map(Text::reference),
        upsert: true,
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.document_api_batch")]
fn a_document_api_save_keeps_chunks_exact() {
    let (rivers, lakes) = (body(4, "rivers"), body(2, "lakes"));
    let mut case = TestCase::from_actions(vec![
        add_article_collection(),
        AddDoc {
            common: Common {
                capture: captures([("article", "/docID")]),
                ..Default::default()
            },
            ..save(None, &rivers)
        }
        .into(),
        chunk_texts_are(&[&rivers], None),
        save(Some("article"), &lakes).into(),
        read_chunks("saved", 2, None),
        chunk_texts_are(&[&lakes], None),
    ]);
    // The CLI has no save command.
    case.client_types = Some(&[ClientType::InProcess, ClientType::RustHttp]);
    case.execute();
}
